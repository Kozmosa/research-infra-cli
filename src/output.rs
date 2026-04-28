use serde::Serialize;

use crate::error::RcliError;

#[derive(Serialize)]
struct ErrorOutput {
    error_code: String,
    message: String,
}

pub fn print_result<T: Serialize>(result: Result<T, RcliError>, json_mode: bool) {
    match result {
        Ok(value) => {
            if json_mode {
                match serde_json::to_string_pretty(&value) {
                    Ok(json) => println!("{}", json),
                    Err(e) => {
                        eprintln!("JSON 序列化错误: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Err(e) => {
            if json_mode {
                let err = ErrorOutput {
                    error_code: e.error_code().to_string(),
                    message: e.to_string(),
                };
                match serde_json::to_string_pretty(&err) {
                    Ok(json) => {
                        eprintln!("{}", json);
                        std::process::exit(1);
                    }
                    Err(_) => {
                        eprintln!(
                            "{{\"error_code\":\"{}\",\"message\":\"序列化失败\"}}",
                            e.error_code()
                        );
                        std::process::exit(1);
                    }
                }
            } else {
                eprintln!("错误 [{}]: {}", e.error_code(), e);
                std::process::exit(1);
            }
        }
    }
}

pub fn print_json<T: Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{}", json),
        Err(e) => {
            eprintln!("JSON 序列化错误: {}", e);
            std::process::exit(1);
        }
    }
}

pub fn print_text(text: &str) {
    println!("{}", text);
}

pub fn print_error(e: &RcliError, json_mode: bool) {
    if json_mode {
        let err = ErrorOutput {
            error_code: e.error_code().to_string(),
            message: e.to_string(),
        };
        match serde_json::to_string_pretty(&err) {
            Ok(json) => eprintln!("{}", json),
            Err(_) => eprintln!(
                "{{\"error_code\":\"{}\",\"message\":\"序列化失败\"}}",
                e.error_code()
            ),
        }
    } else {
        eprintln!("错误 [{}]: {}", e.error_code(), e);
    }
}
