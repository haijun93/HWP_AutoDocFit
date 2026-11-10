use std::env;
fn main() {
 for path in env::args().skip(1) {
   println!("FILE {}", path);
   match pdf_extract::extract_text(&path) { Ok(s) => println!("PDF_EXTRACT={:?}", s), Err(e) => println!("PDF_EXTRACT_ERR={}", e) }
   match lopdf::Document::load(&path) { Ok(doc) => { println!("LOPDF_PAGES={}", doc.get_pages().len()); match doc.extract_text(&doc.get_pages().keys().copied().collect::<Vec<_>>()) { Ok(s)=>println!("LOPDF_TEXT={:?}",s), Err(e)=>println!("LOPDF_ERR={}",e) } }, Err(e)=>println!("LOPDF_LOAD_ERR={}",e) }
 }
}
