use super::*;

#[test]
/// 验证转换导出等待共享预算时仍允许写入端处理其他请求。
fn converted_export_waits_for_shared_budget_without_blocking_the_writer() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.png");
        image::RgbImage::from_pixel(24, 16, image::Rgb([30, 60, 90]))
            .save(&source)
            .unwrap();
        let bytes = std::fs::read(&source).unwrap();
        let mut library = Library::create(dir.path(), "export-budget").unwrap();
        library.import(&source, None).unwrap();
        let asset = library
            .page(&Query::default(), 0, 1)
            .unwrap()
            .assets
            .remove(0);
        let root = library.root().to_path_buf();
        drop(library);
        let service = Service::start().unwrap();
        let id = service.open(root).await.unwrap().info.id;
        let budget = service
            .import_memory
            .clone()
            .acquire_many_owned(4096)
            .await
            .unwrap();
        let worker = service.clone();
        let parent = dir.path().to_path_buf();
        let mut export = tokio::spawn(async move {
            worker
                .export_options(
                    id,
                    vec![asset.id],
                    parent,
                    ExportOptions {
                        format: ExportFormat::Png,
                        ..Default::default()
                    },
                )
                .await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut export)
                .await
                .is_err()
        );
        assert_eq!(
            service
                .call(move |active| Ok(check(active, id)?.info().map_err(|e| e.to_string())?.count))
                .await
                .unwrap(),
            1
        );
        drop(budget);
        let target = tokio::time::timeout(std::time::Duration::from_secs(10), export)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(target.is_dir());
        assert_eq!(std::fs::read(source).unwrap(), bytes);
        service.close().await.unwrap();
    });
}
