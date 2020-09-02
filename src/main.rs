// Heavily lifted from
// https://github.com/bergercookie/asm-lsp
use flexi_logger::{Duplicate, Logger};
use log::{error, info};
use lsp_server::{Connection, Message, Request, RequestId, Response};
use lsp_types::request::{Completion, HoverRequest};
use lsp_types::TextDocumentPositionParams;
use lsp_types::*;
use serde_json::json;
use std::fs::File;
use std::io::BufRead;
// main -------------------------------------------------------------------------------------------
pub fn main() {
    // initialization -----------------------------------------------------------------------------
    // Set up logging. Because `stdio_transport` gets a lock on stdout and stdin, we must have our
    // logging only write out to stderr.
    Logger::with_str("info")
        .log_to_file()
        .duplicate_to_stderr(Duplicate::Warn)
        .start()
        .unwrap_or_else(|e| panic!("Logger initialization failed with {}", e));

    info!("Starting lsp server...");

    // Create the transport
    let (connection, io_threads) = Connection::stdio();

    // Run the server and wait for the two threads to end (typically by trigger LSP Exit event).
    let hover_provider = Some(HoverProviderCapability::Simple(true));
    let completion_provider = Some(CompletionOptions {
        resolve_provider: Some(true),
        trigger_characters: Some(vec![":".to_string(), "(".to_string()]),
        work_done_progress_options: WorkDoneProgressOptions {
            work_done_progress: Some(true),
        },
    });
    let capabilities = ServerCapabilities {
        hover_provider,
        completion_provider,
        ..ServerCapabilities::default()
    };
    info!("{:?}", capabilities);
    let server_capabilities = serde_json::to_value(&capabilities).unwrap();
    let initialization_params = connection
        .initialize(server_capabilities)
        .expect("connect initialize");
    main_loop(&connection, initialization_params);
    io_threads.join().expect("threads");

    // Shut down gracefully.
    info!("Shutting down lsp server");
}
fn empty_response(id: &lsp_server::RequestId) -> Response {
    Response {
        id: id.clone(),
        result: Some(json!("")),
        error: None,
    }
}

fn handle_completion(id: &lsp_server::RequestId, params: &lsp_types::CompletionParams) -> Response {
    // get the word under the cursor
    let word = get_context_from_file_params(params);

    info!("completion word {:?}", &word);
    // get documentation ------------------------------------------------------
    // format response
    let hover_res: Hover;
    match word {
        Ok(word) => {
            hover_res = Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("{}", "this works"),
                }),
                range: None,
            };
            let result = Some(hover_res);
            let result = serde_json::to_value(&result).unwrap();
            //Response {
            //    id: id.clone(),
            //    result: Some(result),
            //   error: None,
            // }
            empty_response(&id)
        }
        Err(_) => {
            // given word is not valid
            empty_response(&id)
        }
    }
}
fn handle_hover(id: &lsp_server::RequestId, params: &lsp_types::HoverParams) -> Response {
    // get the word under the cursor
    let word = get_word_from_file_params(&params.text_document_position_params);

    info!("hover word {:?}", &word);
    // get documentation ------------------------------------------------------
    // format response
    let hover_res: Hover;
    match word {
        Ok(word) => {
            hover_res = Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("{}", "this works"),
                }),
                range: None,
            };
            let result = Some(hover_res);
            let result = serde_json::to_value(&result).unwrap();
            Response {
                id: id.clone(),
                result: Some(result),
                error: None,
            }
        }
        Err(_) => {
            // given word is not valid
            empty_response(&id)
        }
    }
}

fn main_loop(connection: &Connection, params: serde_json::Value) {
    let _params: InitializeParams = serde_json::from_value(params).unwrap();
    info!("Starting LSP loop...");
    for msg in &connection.receiver {
        info!("msg{:?}", &msg);
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req).expect("shutdown") {}

                if let Ok((id, params)) = cast::<Completion>(&req) {
                    connection
                        .sender
                        .send(Message::Response(handle_completion(&id, &params)))
                        .expect("sent");
                };
                if let Ok((id, params)) = cast::<HoverRequest>(&req) {
                    connection
                        .sender
                        .send(Message::Response(handle_hover(&id, &params)))
                        .expect("sent");
                };
            }

            Message::Response(_resp) => {}
            Message::Notification(_notification) => {}
        }
    }
}
/// Represents a text cursor between characters, pointing at the next character in the buffer.
pub type Column = usize;

/// Find the start and end indices of a word inside the given line
/// Borrowed from RLS
pub fn find_word_at_pos(line: &str, col: Column) -> (Column, Column) {
    let line_ = format!("{} ", line);
    let is_ident_char = |c: char| c.is_alphanumeric() || c == '_';

    let start = line_
        .chars()
        .enumerate()
        .take(col)
        .filter(|&(_, c)| !is_ident_char(c))
        .last()
        .map(|(i, _)| i + 1)
        .unwrap_or(0) as usize;

    #[allow(clippy::filter_next)]
    let mut end = line_
        .chars()
        .enumerate()
        .skip(col)
        .filter(|&(_, c)| !is_ident_char(c));

    let end = end.next();
    (start, end.map(|(i, _)| i).unwrap_or(col) as usize)
}

pub fn get_context_from_file_params(params: &CompletionParams) -> Result<String, ()> {
    let uri = params.text_document_position.text_document.uri.clone();
    let line = params.text_document_position.position.line as usize;
    let col = params.text_document_position.position.character as usize;

    let file = File::open(uri.to_file_path()?).expect(&format!("Couldn't open file -> {}", uri));
    let buf_reader = std::io::BufReader::new(file);

    let line_conts = buf_reader.lines().nth(line).unwrap().unwrap();
    let (start, end) = find_word_at_pos(&line_conts, col);
    Ok(String::from(&line_conts[start..end]))
}

pub fn get_word_from_file_params(pos_params: &TextDocumentPositionParams) -> Result<String, ()> {
    let uri = &pos_params.text_document.uri;
    let line = pos_params.position.line as usize;
    let col = pos_params.position.character as usize;

    let file = File::open(uri.to_file_path()?).expect(&format!("Couldn't open file -> {}", uri));
    let buf_reader = std::io::BufReader::new(file);

    let line_conts = buf_reader.lines().nth(line).unwrap().unwrap();
    let (start, end) = find_word_at_pos(&line_conts, col);
    Ok(String::from(&line_conts[start..end]))
}

fn cast<R>(req: &Request) -> Result<(RequestId, R::Params), Request>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    req.clone().extract(R::METHOD)
}
