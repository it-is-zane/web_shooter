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
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("failed to set read timeout");

    let buf_reader = BufReader::new(&mut stream);
    let mut lines = buf_reader.lines();
    let request_line = lines.next().unwrap().unwrap();
    println!("{request_line}");

    // let header_stream = stream.try_clone().unwrap();
    // std::thread::spawn(|| {
    //     let buf_reader = BufReader::new(header_stream);
    //     buf_reader
    //         .lines()
    //         .map_while(Result::ok)
    //         .for_each(|line| println!("{line}"));
    // });

    let path = request_line.split(' ').nth(1).unwrap();

    let not_found = || {
        (
            "HTTP/1.1 404 NOT FOUND",
            file_manager.get("404.html").unwrap(),
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

    let game_header_fix = |response: &mut Vec<u8>| {
        if path.to_lowercase().contains("game") {
            response.extend("Cross-Origin-Embedder-Policy: require-corp\r\n".as_bytes());
            response.extend("Cross-Origin-Opener-Policy: same-origin\r\n".as_bytes());
            response.extend("Cross-Origin-Resource-Policy: cross-origin\r\n".as_bytes());
        }
    };

    let set_mime_type = |response: &mut Vec<u8>| {
        response.extend(
            format!(
                "Content-Type: {} \r\n",
                match path.split('.').next_back() {
                    Some("pck") => "application/octet-stream",
                    Some("wasm") => "application/wasm",
                    Some("js") => "text/javascript",
                    Some("html") => "text/html",
                    Some(_) => "text",
                    None => "",
                }
            )
            .as_bytes(),
        );
    };

    let response = match *contents {
        File::Plain(ref contents) => {
            let length = contents.len();

            println!("{status_line} Content-Length: {length}");

            let mut response =
                format!("{status_line}\r\nContent-Length: {length}\r\n").into_bytes();

            game_header_fix(&mut response);
            set_mime_type(&mut response);

            response.extend("\r\n".as_bytes());
            response.extend(contents);

            response
        }
        File::Br(ref contents) => {
            let length = contents.len();

            println!("{status_line} Content-Length: {length} Content-Encoding: br");

            let mut response =
                format!("{status_line}\r\nContent-Length: {length}\r\nContent-Encoding: br\r\n")
                    .into_bytes();

            game_header_fix(&mut response);
            set_mime_type(&mut response);

            response.extend("\r\n".as_bytes());

            response.extend(contents);

            response
        }
    };

    stream.write_all(&response).unwrap();
    stream.flush().unwrap();
}

enum File {
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
                    .any(|ext| path.ends_with(&ext))
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
            Err(_) => {
                eprintln!("File not found {path}");
                Err(())
            }
        }
    }
}
