use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, RwLock},
};
use web_shooter::ThreadPool;

const PORT: i32 = 7878;

fn main() {
    let listener = TcpListener::bind(format!("[::]:{PORT}")).unwrap();
    let pool = ThreadPool::new(80);

    let file_manager = FileManager::new();

    println!("http://localhost:{PORT}");

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        let file_manager = file_manager.clone();

        pool.execute(move || {
            handle_connection(stream, file_manager);
        })
    }

    println!("Shutting down.")
}

fn handle_connection(mut stream: TcpStream, file_manager: FileManager) {
    let buf_reader = BufReader::new(&mut stream);
    let request_line = buf_reader.lines().next().unwrap().unwrap();
    println!("{request_line}");

    let path = request_line.split(' ').nth(1).unwrap();

    let not_found = || {
        (
            "HTTP/1.1 404 NOT FOUND",
            file_manager.get("res/404.html").unwrap(),
        )
    };

    let (status_line, contents) = match path {
        "/" => match file_manager.get("index.html") {
            Ok(index) => ("HTTP/1.1 200 OK", index),
            Err(_) => not_found(),
        },
        _ => match file_manager.get(path) {
            Ok(item) => ("HTTP/1.1 200 OK", item),
            Err(_) => not_found(),
        },
    };

    let response = match *contents {
        File::None => todo!(),
        File::Plain(ref contents) => {
            let length = contents.len();

            println!("{status_line} Content-Length: {length}");

            let mut response =
                format!("{status_line}\r\nContent-Length: {length}\r\n\r\n").into_bytes();

            response.extend(contents);

            response
        }
        File::Br(ref contents) => {
            let length = contents.len();

            println!("{status_line} Content-Length: {length} Content-Encoding: br");

            let mut response = format!(
                "{status_line}\r\nContent-Length: {length}\r\nContent-Encoding: br\r\n\r\n"
            )
            .into_bytes();

            response.extend(contents);

            response
        }
    };

    stream.write_all(&response).unwrap();
    stream.flush().unwrap();
}

enum File {
    None,
    Plain(Vec<u8>),
    Br(Vec<u8>),
}
#[derive(Clone)]
struct FileManager {
    files: Arc<RwLock<HashMap<String, Arc<File>>>>,
}
impl FileManager {
    fn new() -> Self {
        Self {
            files: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    fn get(&self, path: &str) -> Result<Arc<File>, ()> {
        if let Some(file) = self
            .files
            .read()
            .expect("files RwLock was poisoned")
            .get(path)
        {
            return Ok(file.clone());
        }

        match fs::read("res/".to_owned() + path) {
            Ok(contents) => {
                let file = if ".html .txt .css .js .exe .ttf .otf"
                    .split(' ')
                    .find(|ext| path.ends_with(ext))
                    .is_some()
                {
                    let mut compressor = brotlic::CompressorWriter::new(Vec::new());
                    compressor
                        .write_all(&contents)
                        .expect("Failed to compress {path}");

                    Arc::new(File::Br(
                        compressor.into_inner().expect("Failed to compress {path}"),
                    ))
                } else {
                    Arc::new(File::Plain(contents))
                };

                let mut files = self
                    .files
                    .write()
                    .expect("failed to get write lock on files");

                files.insert(path.to_string(), file.clone());

                Ok(file)
            }
            Err(_) => todo!("File not found {path}"),
        }
    }
}
