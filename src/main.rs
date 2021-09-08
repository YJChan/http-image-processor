use axum::{
    extract::{ContentLengthLimit, Multipart},
    handler::{get},
    response::Html,
    Router,
};
use image::{
    io::Reader as ImageReader, EncodableLayout, ImageError, Rgba,
};
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale};
use serde::Deserialize;
use std::{
    fs::File,
    io::Error,
    io::{Cursor, Read},
    net::SocketAddr,
    path::Path,
};

#[derive(Deserialize, Debug)]
struct WatermarkForm {
    scale: u64,
    text: String,
    posx: u32,
    posy: u32,
}

#[tokio::main]
async fn main() {
    
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "image-processor=debug,tower_http=debug")
    }
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(hello_world))
        .route("/img-watermark", get(show_form).post(watermark_handler));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn show_form() -> Html<&'static str> {
    Html(
        r#"
        <!doctype html>
        <style>
            form {
                padding: 1em;
            }
            input {
                display:block;
                margin: 5px;
            }
            div {
                padding: 5px;
                border: 1px solid #eee;
                width: 250px;
            }
            button {
                width: 250px;
            }
        </style>
        <html>
            <head><title>image processor</title></head>
            <body>
                <form action="/img-watermark" method="post" enctype="multipart/form-data">
                    <h3>Upload file to put watermark</h3>
                    <div>                    
                        <label>
                            Upload file:
                            <input type="file" name="file" multiple>
                        </label>
                    </div>
                    <div>
                        <label>
                            Font Size: 
                            <select name="scale">
                                <option value="14">14</option>
                                <option value="16">16</option>
                                <option value="18">18</option>
                                <option value="20">20</option>
                                <option value="22">22</option>
                                <option value="24">24</option>
                            </select>
                        </label>
                    </div>
                    <div>
                        <label>
                            position x:
                            <input type="number" name="posx"/>
                            position y:
                            <input type="number" name="posy"/>
                        </label>
                    </div>
                    <div>
                        <label>
                            watermark text:
                            <input type="text" name="text"/>
                        </label>
                    </div>
                    <button type="submit">Submit</button>
                </form>
            </body>
        </html>
        "#,
    )
}

async fn hello_world() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

async fn error_page(err_msg: String) -> Html<String> {
    Html(err_msg)
}

// accept 250mb file size
async fn watermark_handler(
    ContentLengthLimit(mut multipart): ContentLengthLimit<Multipart, { 250 * 1024 * 1024 }>,
) -> Html<String> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut scale_num = 18.0;
    let mut posx: u32 = 0;
    let mut posy: u32 = 0;
    let mut text = "Blue Bird".into();
    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        println!("name: {}", name);

        match &*name {
            "file" => {
                let data = field.bytes().await.unwrap();
                bytes = data.to_vec();
                println!("Length of `{}` is {} bytes", name, data.len());
            }
            "scale" => {
                scale_num = match field.text().await.unwrap().parse() {
                    Ok(num) => num,
                    Err(_err) => {
                        return error_page("invalid scale number".into()).await;
                    }
                };
                println!("scale: {}", scale_num);
            }
            "posx" => {
                posx = match field.text().await.unwrap().parse() {
                    Ok(num) => num,
                    Err(_err) => {
                        return error_page(
                            "invalid position x number, only positive number".into(),
                        )
                        .await;
                    }
                };
                println!("posx: {}", posx);
            }
            "posy" => {
                posy = match field.text().await.unwrap().parse() {
                    Ok(num) => num,
                    Err(_err) => {
                        return error_page(
                            "invalid position y number, only positive number".into(),
                        )
                        .await;
                    }
                };
                println!("posy: {}", posy);
            }
            "text" => {
                text = field.text().await.unwrap().to_string();
                println!("text: {}", text);
            }
            _ => println!("processed all form value"),
        }
    }
    let scale = Scale {
        x: scale_num,
        y: scale_num,
    };

    let watermarked_img = match draw_watermark_on_image(bytes, scale, &text, posx, posy) {
        Ok(i) => i,
        Err(err) => {
            println!("error when drawing on image, {:?}", err);
            return Html("<h1>Image type is not supported</h1>".into());
        }
    };

    let base64_img = base64::encode(watermarked_img);

    let html_resp = format!(
        r#"
        <!doctype html>
        <html>
            <head><title>image processor</title></head>
            <body>
                <h3>Output:</h3>
                <div style="border: 1px solid #eee; width: min-content; padding: 5px;">
                <img src="data:image/jpg;base64, {}"/>
                </div>
            </body>
        </html>
    "#,
        base64_img
    );

    Html(html_resp)
}

fn read_image(path: &str) -> Result<Vec<u8>, Error> {
    let mut file = File::open(Path::new(path))?;
    let mut buff = Vec::new();
    file.read_to_end(&mut buff)?;

    Ok(buff)
}

fn determine_image_format(img: Vec<u8>) {
    let cursor = Cursor::new(img.as_bytes());
    let reader = ImageReader::new(cursor)
        .with_guessed_format()
        .expect("never failed this");
    println!("format guessed: {:?}", reader.format());
}

fn draw_watermark_on_image(
    img: Vec<u8>,
    scale: Scale,
    text: &str,
    posx: u32,
    posy: u32,
) -> Result<Vec<u8>, ImageError> {
    let cursor = Cursor::new(img.as_bytes());
    let mut dyna_img = match ImageReader::new(cursor).with_guessed_format()?.decode() {
        Ok(i) => i,
        Err(err) => {
            println!("image cannot, {:?}", err);
            return Err(err);
        }
    };

    let font_data: &[u8] = include_bytes!("../fonts/Urbanist/static/Urbanist-Black.ttf");
    let font: Font<'static> = match Font::try_from_bytes(font_data) {
        Some(f) => f,
        None => {
            println!("font error");
            return Err(ImageError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                "font error",
            )));
        }
    };
    // let scale: Scale = Scale { x: 18.0, y: 18.0 };
    // let text = "IBM Technology Garage";
    let color = Rgba([0u8, 0u8, 0u8, 0u8]);
    // let x = dyna_img.width() - posx as u32;
    // let y = dyna_img.height() - posy as u32;
    draw_text_mut(&mut dyna_img, color, posx, posy, scale, &font, text);

    // save to local
    // dyna_img.save("images/kubernetes-watermarked.jpg").unwrap();

    // save in memory
    let mut out_img = Vec::new();
    dyna_img
        .write_to(&mut out_img, image::ImageFormat::Jpeg)
        .expect("writing to memory failed");

    Ok(out_img)
}
