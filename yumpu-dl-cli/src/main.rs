use std::path::PathBuf;
use std::process::exit;
use std::sync::Mutex;

use yumpu_dl_lib::{download_yumpu_to_pdf, Logger};

struct SimpleLogger {
    completed: Mutex<u64>,
    operations: Mutex<u64>,
}

impl Logger for SimpleLogger {
    fn is_initialized(&self) -> bool {
        let mtx_ops = self.operations.lock().unwrap();
        *mtx_ops > 0
    }

    fn set_total_operations(&self, ops: u64) {
        let mut mtx_ops = self.operations.lock().unwrap();
        *mtx_ops = ops;
    }

    fn increment_progression(&self) {
        let mtx_ops = self.operations.lock().unwrap();
        let mut mtx_comp = self.completed.lock().unwrap();
        *mtx_comp += 1;
        println!("Progress: [{}/{}]", *mtx_comp, *mtx_ops);
    }

    fn log_message(&self, msg: &str) {
        println!("Message: {}", msg);
    }
}

#[tokio::main]
async fn main() {
    let params: Vec<_> = std::env::args().collect();
    if params.len() != 3 {
        println!("This program takes 2 arguments: yumpu-dl-cli <yumpu-url> <target-file>");
        exit(0);
    }

    let url = params[1].to_string();
    let target = PathBuf::from(params[2].as_str());
    let logger = SimpleLogger { completed: Mutex::new(0), operations: Mutex::new(0) };

    download_yumpu_to_pdf(&url, &target, Option::Some(&logger)).await.unwrap()
}
