use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::Read;
use std::rc::Rc;

use crate::{ascii_util, tok_maps};
use crate::cbuf::CBuf;
use crate::ident::Ident;
use crate::shared::shared_ptr;
use crate::sloc::SourceLoc;
use crate::tok_flags::{IS_AT_BOL, LF_AFTER, USER_DEFINED_ID_BEGIN_UID, WS_BEFORE};
use crate::tok_maps::Keywords;
use crate::token::Token;
use crate::toktype::T;

pub struct Tokenizer {
    file_name: Rc<String>,
    buffer: CBuf,
    punct_map: HashMap<&'static str, T>,
    idmap: HashMap<String, shared_ptr<Ident>>,
}

impl Tokenizer {
    pub fn new_from_file(file_name: String, idmap: HashMap<String, shared_ptr<Ident>>) -> Self {
        let content = read_file(&file_name);
        let mut punct_map = tok_maps::make_maps();

        Tokenizer {
            file_name: Rc::new(file_name),
            buffer: CBuf::create(&content),
            punct_map,
            idmap,
        }
    }

    pub fn new_from_string(content: String, idmap: HashMap<String, shared_ptr<Ident>>) -> Self {
        let maps = tok_maps::make_maps();
        let mut punct_map = tok_maps::make_maps();

        Tokenizer {
            file_name: Rc::new("<string-input>".to_string()),
            buffer: CBuf::create(&content),
            punct_map,
            idmap,
        }
    }

    fn create_token(&self, tp: T, sb: &String) -> Token {
        return Token::new(tp.clone()
                          , sb.clone()
                          , self.build_sloc(&sb.clone()),
        );
    }

    fn create_token_spec_loc(&self, tp: T, sb: &String, loc: SourceLoc) -> Token {
        return Token::new(tp.clone(), sb.clone(), loc);
    }

    pub fn next(&mut self) -> Token
    {
        let mut buffer = &mut self.buffer;

        let begin = buffer.peek_4();
        let c1 = begin[0];
        let c2 = begin[1];
        let c3 = begin[2];
        let c4 = begin[3];

        // whitespace, newline, EOF

        if c1 == b'\0' {
            return Token::make_eof();
        }

        // TODO: unicode whitespaces
        if c1 == b' ' || c1 == b'\t' {
            buffer.next();
            return Token::make_ws();
        }

        if c1 == b'\n' {
            buffer.next();
            return Token::make_lf();
        }

        // comments // and /**/
        // TODO: doc.comments, begin location for error handling.
        if c1 == b'/' {
            if c2 == b'/' {
                let mut comments = String::new();
                comments.push_str("//");

                buffer.next();
                buffer.next();

                while !buffer.is_eof() {
                    let tmp = buffer.next();
                    if tmp == b'\n' {
                        // TODO: doc. comments
                        // return Token::new(T::TOKEN_COMMENT, comments, sloc);
                        return Token::make_lf();
                    }

                    if tmp == b'\0' {
                        panic!("no new-line at end of file..."); // TODO: location here
                    }

                    comments.push(tmp as char);
                }
            } else if c2 == b'*' {
                buffer.next();
                buffer.next();

                let mut prev = b'\0';
                while !buffer.is_eof() {
                    let tmp = buffer.next();
                    if tmp == b'\0' {
                        panic!("unclosed comment"); // TODO: location here
                    }
                    if tmp == b'/' && prev == b'*' {
                        return Token::make_ws();
                    }
                    prev = tmp;
                }
            }
        }

        // identifiers

        if ascii_util::is_letter(c1) {
            let mut sb = String::new();

            while !buffer.is_eof() {
                let peek1 = buffer.peek_1();
                let is_identifier_tail = ascii_util::is_letter(peek1) || ascii_util::is_dec(peek1);
                if !is_identifier_tail {
                    break;
                }
                sb.push(buffer.next() as char);
            }

            // Put the identifier we found in the hash.
            //
            // All identifiers are shared between tokens.
            // Each identifier is actually a unique pointer.
            // For example: we have a loop in its simple form: for(int i=0; i<10; i+=1) {}
            // The 'i' as an identifier will be presented in the hash once.
            // The 'i' as a token will be presented three times, and each token will has a ref
            // to the 'i' identifier, which is unique through the whole program, and contains a
            // useful information about the 'named-identifier'. It may be a keyword, it may be a
            // macro-name, it may be a special symbol, etc... So: we do not have to store somewhere
            // a special hash-table for names if we can bind each name with a token in the
            // token-tree. This simple trick works fine in C, with a raw-pointers, where we can
            // compare these identifiers as pointers, and not as strings.
            //
            if !self.idmap.contains_key(&sb) {
                let id = Ident::new(sb.clone());
                self.idmap.insert(sb.clone(), shared_ptr::new(id));
            }

            return self.create_token(T::TOKEN_IDENT, &sb);
        }

        // operators

        if ascii_util::is_op_start(c1) {

            // 4
            let mut four = String::from(c1 as char);
            four.push(c2 as char);
            four.push(c3 as char);
            four.push(c4 as char);

            if self.punct_map.contains_key(four.as_str()) {
                buffer.next();
                buffer.next();
                buffer.next();
                buffer.next();

                let tp = self.punct_map.get(four.as_str()).unwrap();
                return self.create_token(tp.clone(), &four);
            }

            // 3
            let mut three = String::from(c1 as char);
            three.push(c2 as char);
            three.push(c3 as char);

            if self.punct_map.contains_key(three.as_str()) {
                buffer.next();
                buffer.next();
                buffer.next();

                let tp = self.punct_map.get(three.as_str()).unwrap();
                return self.create_token(tp.clone(), &three);
            }

            // 2
            let mut two = String::from(c1 as char);
            two.push(c2 as char);

            if self.punct_map.contains_key(two.as_str()) {
                buffer.next();
                buffer.next();

                let tp = self.punct_map.get(two.as_str()).unwrap();
                return self.create_token(tp.clone(), &two);
            }

            // 1
            let mut one = String::from(c1 as char);

            if self.punct_map.contains_key(one.as_str()) {
                buffer.next();

                let tp = self.punct_map.get(one.as_str()).unwrap();
                return self.create_token(tp.clone(), &one);
            }

            panic!("unknown operator {}", three); // TODO: location here
        }

        // numbers
        // TODO: here we have to handle range patterns: 0..10, 0..=10, etc...
        if ascii_util::is_dec(c1) {
            let mut sb = String::new();

            while !buffer.is_eof() {
                let mut peekc = buffer.peek_1();
                if ascii_util::is_dec(peekc) {
                    sb.push(buffer.next() as char);
                    continue;
                } else if peekc == b'e' || peekc == b'E' || peekc == b'p' || peekc == b'P' {
                    sb.push(buffer.next() as char);

                    peekc = buffer.peek_1();
                    if peekc == b'-' || peekc == b'+' {
                        sb.push(buffer.next() as char);
                    }
                    continue;
                } else if peekc == b'.' || ascii_util::is_letter(peekc) {
                    sb.push(buffer.next() as char);
                    continue;
                }

                break;
            }

            return self.create_token(T::TOKEN_NUMBER, &sb);
        }

        // string, char
        // TODO: here we have to handle lifetime patterns: 'a, 'static, etc...
        if c1 == b'\"' || c1 == b'\'' {
            let end = buffer.next(); // skip the quote

            let line = buffer.line;
            let column = buffer.column;
            let loc = SourceLoc::new(Rc::clone(&self.file_name), line, column);

            let mut sb = String::new();
            while !buffer.is_eof() {
                let next = buffer.next();

                if next == b'\0' {
                    panic!("unclosed string"); // TODO: location here
                }
                if next == b'\n' {
                    // panic!("end of line in string");
                }
                if next == end {
                    break;
                }

                if next == b'\\' {
                    // escaped character
                    sb.push_str("\\");
                    sb.push(buffer.next() as char);
                } else {
                    // normal symbol
                    sb.push(next as char);
                }
            }

            // string

            let mut repr = String::from(end as char);
            repr.push_str(&sb.clone());
            repr.push(end as char);

            if end == b'\"' {
                return self.create_token_spec_loc(T::TOKEN_STRING, &repr, loc);
            }

            return self.create_token_spec_loc(T::TOKEN_CHAR, &repr, loc);
        }

        // other ASCII
        let mut one = String::from(c1 as char);
        if self.punct_map.contains_key(one.as_str()) {
            buffer.next();
            let tp = self.punct_map.get(one.as_str()).unwrap();
            return self.create_token(tp.clone(), &one);
        }

        // we do not really know what this char means
        let unknown = String::from(c1 as char);
        buffer.next(); // XXX
        return self.create_token(T::TOKEN_ERROR, &unknown);
    }


    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokenlist: Vec<Token> = Vec::new();

        let mut line: Vec<Token> = Vec::new();
        let mut next_ws = false;

        while !self.buffer.is_eof() {
            let mut t = self.next();

            if t.is(T::TOKEN_IDENT) {
                let opt = self.idmap.get(&t.val);
                if opt.is_none() {
                    panic!("cannot find the name `{}` in the hash-table", &t.val);
                }

                let x = opt.unwrap();
                t.id = Option::from(shared_ptr::_cloneref(x));
            }

            if t.is(T::TOKEN_EOF) {
                for tok in line {
                    tokenlist.push(tok);
                }
                tokenlist.push(t); // EOF itself
                break;
            }

            if next_ws {
                t.pos |= WS_BEFORE;
                next_ws = false;
            }

            if t.is(T::TOKEN_LF) || t.is(T::TOKEN_COMMENT) {
                if t.is(T::TOKEN_COMMENT) {
                    line.push(t);
                }
                if line.is_empty() {
                    continue;
                }

                // Here we have to set all of the flags for the first and the last tokens in the line.
                // We know that the line is not empty, so: unwrap() is safety here.

                let len = line.len();
                let mut last = line.get_mut(len - 1).unwrap();
                last.pos |= LF_AFTER;

                let mut first = line.get_mut(0).unwrap();
                first.pos |= IS_AT_BOL;
                first.pos |= WS_BEFORE;

                for tok in line {
                    tokenlist.push(tok);
                }
                line = Vec::new();
                continue;
            }

            if t.is(T::TOKEN_WS) {
                next_ws = true;
                continue;
            }

            line.push(t);
        }

        println!("map len: {}", self.idmap.len());
        return tokenlist;
    }

    fn build_sloc(&self, sb: &String) -> SourceLoc {
        let mut col = self.buffer.column;
        let len = sb.len() as i32;
        let mut offs = self.buffer.column as i32;
        if col >= len {
            offs = col - len + 1;
        }
        let fname = Rc::clone(&self.file_name);
        return SourceLoc::new(fname, self.buffer.line, offs);
    }
}


fn read_file(filename: &str) -> String {
    let mut input = String::new();
    let mut fp = io::stdin();
    let mut fp = File::open(filename).expect("file not found");
    fp.read_to_string(&mut input)
        .expect("an internal error, cannot read the file");
    return input;
}