use std::collections::{HashMap, VecDeque};
use std::fs::{read_dir, read_to_string, File, ReadDir};
use std::path::{Path, PathBuf};

use crate::commons::{exit_error, Lexer};
use crate::database::models::doc_word::DbDocWordToSave;
use crate::database::models::document::DbDocument;
use crate::database::models::word::DbWord;
use crate::database::sqlite::Sqlite;
use crate::database::DataBase;

pub struct Document {
	pub count_words: i32,
	pub path: PathBuf,
	pub term_freq: HashMap<String, (i32, f32)>,
}

impl Document {
	fn populate_term_freq(&mut self, text: &str) {
		let chars: Vec<char> = text.chars().collect();

		let lexer = Lexer::new(&chars);

		for word in lexer {
			match self.term_freq.get_mut(&word) {
				Some(value) => value.0 += 1,
				None => {
					self.term_freq.insert(word.to_owned(), (1, 0.0));
				}
			};

			self.count_words += 1;
		}
	}

	pub fn new(text: &str, path: PathBuf) -> Self {
		let mut doc = Document {
			count_words: 0,
			path,
			term_freq: HashMap::new(),
		};

		doc.populate_term_freq(text);

		let freq = doc.term_freq.clone();

		for (key, value) in freq.iter() {
			doc.term_freq.insert(
				key.clone(),
				(value.0, (value.0 as f32) / (doc.count_words as f32)),
			);
		}

		doc
	}

	pub fn save(self, db: &mut impl DataBase) {
		let db_doc = DbDocument {
			id: 0,
			total_terms: self.count_words as usize,
			path: self.path.display().to_string(),
		};

		let last_doc_id = db.save(db_doc);

		// println!("");
		// println!("Save: {} -> {}", last_doc_id, self.path.display());
		// println!("");

		let words: Vec<DbWord> = self
			.term_freq
			.clone()
			.into_keys()
			.map(|w| DbWord {
				id: 0,
				word: w,
				apparitions: 1,
			})
			.collect();

		// println!("");
		// println!("Words: {}", words.len());
		// println!("");

		db.save_all(
			&words,
			String::from("T_WORD"),
			String::from("ON CONFLICT(word) DO UPDATE SET apparitions = apparitions + 1"),
		);

		let doc_words: Vec<DbDocWordToSave> = self
			.term_freq
			.clone()
			.into_iter()
			.map(|(word, (a, tf))| DbDocWordToSave {
				word,
				id_doc: last_doc_id,
				apparition: a as usize,
				tf,
			})
			.collect();

		db.save_all_doc_word(&doc_words);
	}
}

pub fn index_from_str(text: &str, path: &Path) -> Option<Document> {
	let doc = Document::new(text, path.to_path_buf());

	if doc.count_words == 0 {
		return None;
	}

	// println!("");
	// println!("Document words: {}", doc.count_words);
	// println!("");

	Some(doc)
}

pub fn is_valid_file(path: &Path) -> bool {
	let files_types = ["pdf", "txt"];
	let file_extension = path.extension();

	match file_extension {
		Some(ext) => files_types.contains(&ext.to_str().unwrap()),
		_ => false,
	}
}

pub fn extract_pdf(path: &PathBuf) -> Option<String> {
	use poppler::Document;
	use std::io::Read;

	let mut content = Vec::new();
	File::open(path)
		.and_then(|mut file| file.read_to_end(&mut content))
		.map_err(|err| {
			eprintln!(
				"ERROR: could not read file {file_path}: {err}",
				file_path = path.display()
			);
		})
		.expect("Error trying to read file");

	let pdf = Document::from_data(&content, None).unwrap();

	let mut result = String::new();

	let n = pdf.n_pages();
	for i in 0..n {
		let page = pdf
			.page(i)
			.unwrap_or_else(|| panic!("{i} is within the bounds of the range of the page"));
		if let Some(content) = page.text() {
			result.push_str(content.as_str());
			result.push(' ');
		}
	}

	// println!("");
	// println!("Content length: {}", result.len());
	// println!("");
	if result.is_empty() {
		None
	} else {
		Some(result)
	}
}

pub fn extract_txt(path: &PathBuf) -> Option<String> {
	match read_to_string(path) {
		Ok(content) => Some(content),
		Err(_) => None,
	}
}

pub fn calc_idf(term: &str, docs: &Vec<Document>) -> f32 {
	let total_documents = docs.len() as f32;

	let mut appears = 1;

	for doc in docs {
		if doc.term_freq.contains_key(term) {
			appears += 1;
		}
	}
	let semi = total_documents / (appears as f32);

	semi.log10().max(1f32)
}

pub fn extract(dir: ReadDir, docs: &mut Vec<Document>) {
	for item in dir {
		let item = item.unwrap().path();
		print!("Try: {} ", item.display());

		if item.is_dir() {
			extract(read_dir(&item).unwrap(), docs);
		} else if is_valid_file(&item) {
			let content = match item.extension().unwrap().to_str().unwrap() {
				"pdf" => extract_pdf(&item),
				"txt" => extract_txt(&item),
				_ => None,
			};

			let doc: Option<Document> = match content {
				Some(content) => index_from_str(&content, &item),
				None => None,
			};

			if let Some(doc) = doc {
				println!("success!!!");
				docs.push(doc);
			} else {
				println!("Can't get content from: {}", item.display());
			}
		}
	}
}

pub fn index(mut args: VecDeque<String>) {
	let path = args.pop_front().unwrap_or(String::from("files"));
	let db_name = args.pop_front().unwrap_or(String::from("indexed.db"));

	let mut db = Sqlite::new(db_name);

	let mut docs: Vec<Document> = Vec::new();

	match read_dir(path) {
		Ok(items) => {
			extract(items, &mut docs);
			for doc in docs {
				// for (word, (freq, tf)) in doc.term_freq.clone().iter() {
				// 	println!("{} => {} - {} - {}", doc.path.display(), word, freq, tf);
				// }
				doc.save(&mut db);
			}
		}
		Err(_) => exit_error("The argument must be a directory"),
	}
}
