use serde::{Deserialize, Serialize};
use worker::{kv::Key, *};

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
        let os_len = os.len();
        return match ctx
            .kv("worker-dynamic-quickemu")?
            .list()
            .prefix(os + "-")
            .execute()
            .await
        {
            Ok(list) if !list.keys.is_empty() => {
                let response_list = List {
                    keys: list.keys,
                    os_len,
                };
                Response::from_json(&response_list)
            }
            _ => Response::error("No values for selected OS", 400),
        };
    }
    Response::error("Bad Request", 400)
}

struct List {
    keys: Vec<Key>,
    os_len: usize,
}

impl Serialize for List {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let iter = self.keys.iter().filter_map(|key| {
            key.metadata
                .clone()
                .and_then(|metadata| serde_json::from_value(metadata).ok())
                .map(|metadata: Metadata| {
                    let status = if let Some(error) = metadata.error {
                        ListStatus::Error { error }
                    } else {
                        // All keys are prefixed by '${os}-', this should never panic
                        let os = &key.name[..self.os_len];
                        let denom = &key.name[self.os_len + 1..];

                        ListStatus::Valid {
                            url: format!("./downloadRedirect?os={os}&denom={denom}",),
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
        });
        serializer.collect_seq(iter)
    }
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

#[derive(Serialize)]
struct ListEntry {
    release: String,
    edition: Option<String>,
    arch: String,
    #[serde(flatten)]
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
