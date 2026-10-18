//! Constant-time fixed-grid viewport calculation. Independent of DB and GPU.
use std::ops::Range;

#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub index: usize,
    pub top: f32,
    pub height: f32,
}
#[derive(Debug)]
pub struct MasonryIndex {
    pub columns: Vec<Vec<Placement>>,
    pub width: f32,
    pub height: f32,
    pub count: usize,
    pub widths: Vec<f32>,
}
impl MasonryIndex {
    pub fn new(sizes: &[(u32, u32)], columns: usize, width: f32, gap: f32) -> Self {
        let width = width.max(1.0);
        let gap = gap.max(0.0);
        let count = columns.clamp(1, 12);
        let mut result = vec![Vec::new(); count];
        let mut bottoms = vec![0.0f32; count];
        let widths = if count == 4 {
            [107.0, 87.0, 94.0, 100.0]
                .into_iter()
                .map(|weight| width * weight / 97.0)
                .collect::<Vec<_>>()
        } else {
            vec![width; count]
        };
        for (index, &(w, h)) in sizes.iter().enumerate() {
            let column = bottoms
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.total_cmp(b.1))
                .map(|(i, _)| i)
                .unwrap_or(0);
            let tile_width = widths[column];
            let height = (tile_width * h as f32 / w.max(1) as f32)
                .clamp(tile_width * 0.45, tile_width * 1.28);
            result[column].push(Placement {
                index,
                top: bottoms[column],
                height,
            });
            bottoms[column] += height + gap;
        }
        let height = bottoms.into_iter().fold(0.0, f32::max).max(gap) - gap;
        Self {
            columns: result,
            width,
            height,
            count: sizes.len(),
            widths,
        }
    }
    pub fn visible(&self, top: f32, height: f32) -> Vec<Vec<Placement>> {
        let start = (top - self.width * 1.5).max(0.0);
        let end = top + height + self.width * 1.5;
        self.columns
            .iter()
            .map(|column| {
                let first = column.partition_point(|item| item.top + item.height < start);
                let last = column.partition_point(|item| item.top < end);
                column[first..last].to_vec()
            })
            .collect()
    }
    pub fn page_range(&self, top: f32, height: f32) -> Range<usize> {
        let visible = self.visible(top, height);
        let mut indices = visible.iter().flatten().map(|p| p.index);
        let Some(first) = indices.next() else {
            return 0..0;
        };
        let (min, max) = indices.fold((first, first), |(min, max), index| {
            (min.min(index), max.max(index))
        });
        min..max + 1
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Grid {
    pub columns: usize,
    pub row_height: f32,
    pub viewport_height: f32,
}
impl Grid {
    pub fn visible(self, total: usize, scroll_y: f32) -> Range<usize> {
        let columns = self.columns.max(1);
        let height = self.row_height.max(1.0);
        let first_row = (scroll_y.max(0.0) / height) as usize;
        let start = first_row
            .saturating_sub(2)
            .saturating_mul(columns)
            .min(total);
        let rows = (self.viewport_height.max(0.0) / height).ceil() as usize + 5;
        start
            ..start
                .saturating_add(rows.saturating_mul(columns))
                .min(total)
    }
    pub fn total_height(self, total: usize) -> f32 {
        total.div_ceil(self.columns.max(1)) as f32 * self.row_height
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn random_jumps_keep_widget_budget_constant() {
        let grid = Grid {
            columns: 4,
            row_height: 220.0,
            viewport_height: 900.0,
        };
        for total in [1_000, 10_000, 100_000, 500_000] {
            for index in [0, total / 2, total - 1] {
                let range = grid.visible(total, (index / 4) as f32 * 220.0);
                assert!(range.len() <= 40);
                assert!(range.end <= total);
                assert!(range.contains(&index));
            }
        }
        assert!(grid.visible(0, 0.0).is_empty());
    }
    #[test]
    fn masonry_random_jumps_match_brute_force_and_never_overlap() {
        for count in [10_000, 100_000, 500_000] {
            let sizes: Vec<_> = (0..count)
                .map(|i| (400 + (i % 5) * 80, 240 + (i % 7) * 90))
                .collect();
            let layout = MasonryIndex::new(&sizes, 4, 220.0, 10.0);
            assert_eq!(
                layout.columns.iter().map(Vec::len).sum::<usize>(),
                count as usize
            );
            for column in &layout.columns {
                assert!(
                    column
                        .windows(2)
                        .all(|pair| pair[0].top + pair[0].height + 9.9 <= pair[1].top)
                );
            }
            for scroll in [0.0, layout.height * 0.5, (layout.height - 800.0).max(0.0)] {
                let actual = layout.visible(scroll, 800.0);
                let low = (scroll - layout.width * 1.5).max(0.0);
                let high = scroll + 800.0 + layout.width * 1.5;
                for (expected, actual) in layout.columns.iter().zip(actual.iter()) {
                    let expected: Vec<_> = expected
                        .iter()
                        .filter(|p| p.top + p.height >= low && p.top < high)
                        .map(|p| p.index)
                        .collect();
                    assert_eq!(expected, actual.iter().map(|p| p.index).collect::<Vec<_>>());
                }
                assert!(actual.iter().map(Vec::len).sum::<usize>() <= 80);
                let range = layout.page_range(scroll, 800.0);
                assert!(range.end <= count as usize);
                assert!(actual.iter().flatten().all(|p| range.contains(&p.index)));
            }
        }
    }
    #[test]
    fn masonry_empty_and_invalid_dimensions_are_bounded() {
        let empty = MasonryIndex::new(&[], 0, 0.0, 0.0);
        assert_eq!(empty.height, 0.0);
        assert_eq!(empty.page_range(0.0, 800.0), 0..0);
        let layout = MasonryIndex::new(&[(0, 0), (1, u32::MAX), (u32::MAX, 1)], 4, 220.0, 10.0);
        assert!(
            layout
                .columns
                .iter()
                .flatten()
                .all(|p| p.height.is_finite() && p.height > 0.0 && p.height < 1000.0)
        );
    }
}
