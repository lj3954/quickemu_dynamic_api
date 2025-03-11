use serde::{Deserialize, Serialize};
use worker::*;

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let router = Router::new();

    router
        .get_async("/list", list)
        .get_async("/downloadRedirect", download_redirect)
        .run(req, env)
        .await
}

async fn download_redirect(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if let Ok(DownloadParameters { os, denom }) = req.query() {
        return match ctx
            .kv("worker-dynamic-quickemu")?
            .get(&(os + "-" + &denom))
            .json::<Value>()
            .await
        {
            Ok(Some(value)) => match value {
                Value::Success { url } => match Url::parse(&url) {
                    Ok(url) => Response::redirect(url),
                    _ => Response::error("An unknown error occurred", 400),
                },
                Value::Failure { error } => Response::error(error, 400),
            },
            _ => Response::error("Could not find a matching entry", 400),
        };
    }
    Response::error("Bad Request", 400)
}

#[derive(Deserialize)]
#[serde(tag = "status")]
enum Value {
    Success { url: String },
    Failure { error: String },
}

#[derive(Deserialize)]
struct DownloadParameters {
    os: String,
    denom: String,
}

async fn list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if let Ok(ListParameters { os }) = req.query() {
        return match ctx
            .kv("worker-dynamic-quickemu")?
            .list()
            .prefix(os.clone() + "-")
            .execute()
            .await
        {
            Ok(list) if !list.keys.is_empty() => {
                let list: List = list
                    .keys
                    .into_iter()
                    .filter_map(|key| {
                        key.metadata
                            .and_then(|metadata| serde_json::from_value(metadata).ok())
                            .map(|metadata: Metadata| {
                                let status = if let Some(error) = metadata.error {
                                    ListStatus::Error { error }
                                } else {
                                    ListStatus::Valid {
                                        url: format!(
                                            "./downloadRedirect?os={os}&denom={}",
                                            &key.name[os.len() + 1..]
                                        ),
                                        filename: metadata.filename.unwrap_or_default(),
                                        checksum: metadata.checksum,
                                    }
                                };
                                ListEntry {
                                    release: metadata.release,
                                    edition: metadata.edition,
                                    arch: metadata.arch,
                                    status,
                                }
                            })
                    })
                    .collect();
                Response::from_json(&list)
            }
            _ => Response::error("No values for selected OS", 400),
        };
    }
    Response::error("Bad Request", 400)
}

#[derive(Deserialize)]
struct Metadata {
    release: String,
    arch: String,
    edition: Option<String>,
    filename: Option<String>,
    checksum: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct ListParameters {
    os: String,
}

type List = Vec<ListEntry>;

#[derive(Serialize)]
struct ListEntry {
    release: String,
    edition: Option<String>,
    arch: String,
    status: ListStatus,
}

#[derive(Serialize)]
#[serde(tag = "status")]
enum ListStatus {
    Valid {
        url: String,
        filename: String,
        checksum: Option<String>,
    },
    Error {
        error: String,
    },
}
