//! Isolated sample data for reviewing person cards and dialogs, without inference.
use moriq_domain::*;
use rusqlite::params;

/// 只在显式隔离模式下准备人物界面样例，不访问用户资源库。
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("MORIQ_ISOLATED_LIFECYCLE_PROBE").as_deref() != Ok("1") {
        return Err("isolated settings required".into());
    }
    let parent = std::env::args_os()
        .nth(1)
        .ok_or("fixture parent required")?;
    let assets =
        std::path::PathBuf::from(std::env::args_os().nth(2).ok_or("asset folder required")?);
    std::fs::create_dir_all(&parent)?;
    let mut lib = moriq_library::Library::create(std::path::Path::new(&parent), "人物交互样例")?;
    let project = lib.create_project("人像样例")?;
    for n in 1..=6 {
        lib.import(&assets.join(format!("photo-{n:02}.png")), Some(project))?;
    }
    lib.set_cover(&assets.join("photo-01.png"))?;
    lib.set_workspace(Some(project));
    let photos = lib.page(&Query::default(), 0, 10)?.assets;
    let root = lib.root().to_path_buf();
    drop(lib);
    let db = rusqlite::Connection::open(root.join("catalog.db"))?;
    for (i, photo) in photos.iter().enumerate() {
        let person = format!("sample-person-{i}");
        db.execute(
            "INSERT INTO people VALUES(?1,?2,'ui-sample',0)",
            params![person, format!("示例人物 {}", i + 1)],
        )?;
        db.execute(
            "INSERT INTO workspace_people VALUES(?1,?2)",
            params![person, project.to_string()],
        )?;
        db.execute("INSERT INTO detected_faces(id,version_id,revision,ordinal,box,vector,person_id) VALUES(?1,?2,'ui-sample',0,'[0.1,0.1,0.7,0.7]',x'0000803f',?3)",params![format!("sample-face-{i}"),photo.version_id.to_string(),person])?;
        db.execute("INSERT INTO ai_results VALUES(?1,'faces','ui-sample','srgb-1024-v1','{\"faces\":[]}',0)",[photo.version_id.to_string()])?;
    }
    println!("{}", root.display());
    Ok(())
}
