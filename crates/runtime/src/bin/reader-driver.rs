use anyhow::{Context, Result, bail};
use reader_core::BookId;
use reader_runtime::{AppPaths, Library};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

fn main() -> Result<()> {
    let root = std::env::args().nth(1).context("需要隔离测试目录")?;
    let paths = if root == "--native" {
        AppPaths::native()?
    } else {
        AppPaths::isolated(&PathBuf::from(root))?
    };
    let mut library = Library::open(paths)?;
    for line in io::stdin().lock().lines() {
        let result = line
            .map_err(anyhow::Error::from)
            .and_then(|line| serde_json::from_str::<Value>(&line).map_err(Into::into))
            .and_then(|command| execute(&mut library, command));
        let response = match result {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(e) => json!({"ok":false,"error":e.to_string()}),
        };
        println!("{response}");
        io::stdout().flush()?;
    }
    Ok(())
}
fn execute(library: &mut Library, c: Value) -> Result<Value> {
    let id = || c["id"].as_i64().map(BookId).context("需要书籍 ID");
    Ok(match c["op"].as_str().context("缺少操作")? {
        "paths" => serde_json::to_value(&library.paths)?,
        "linux_paths" => serde_json::to_value(AppPaths::linux(
            std::path::Path::new(c["home"].as_str().context("缺少 home")?),
            &serde_json::from_value(c["env"].clone())?,
        )?)?,
        "import" => serde_json::to_value(
            library.import(&serde_json::from_value::<Vec<PathBuf>>(c["paths"].clone())?),
        )?,
        "books" => serde_json::to_value(library.books()?)?,
        "open" => {
            library.record_open(id()?)?;
            Value::Null
        }
        "rename" => {
            serde_json::to_value(library.rename(id()?, c["title"].as_str().context("缺少书名")?)?)?
        }
        "delete" => {
            library.delete(id()?)?;
            Value::Null
        }
        "position" => {
            library.save_location(id()?, &serde_json::from_value(c["location"].clone())?)?;
            Value::Null
        }
        "settings" => {
            if let Some(value) = c.get("value") {
                library.save_settings(&serde_json::from_value(value.clone())?)?;
            }
            serde_json::to_value(library.settings()?)?
        }
        "workspace" => {
            if let Some(value) = c.get("value") {
                library.save_workspace(&serde_json::from_value(value.clone())?)?;
            }
            serde_json::to_value(library.workspace()?)?
        }
        "epub" | "destination" => {
            let book = library.book(id()?)?;
            let doc = reader_runtime::epub::EpubBook::open(&library.path(&book))?;
            if c["op"] == "destination" {
                serde_json::to_value(doc.destination(c["href"].as_str().context("缺少 href")?))?
            } else {
                json!({"chapters":doc.chapters,"toc":doc.toc,"images":doc.images.len(), "image_hrefs":doc.images.keys().collect::<Vec<_>>()})
            }
        }
        _ => bail!("未知操作"),
    })
}
