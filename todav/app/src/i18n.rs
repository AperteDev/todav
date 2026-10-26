//! Translations from the `todav` gettext catalogs (`po/`, compiled to `.mo` by msgfmt), for the
//! front-ends without glibc's gettext (Android, Windows): the same messages, contexts and plural
//! rules as the Linux app. The front-end picks the language and hands over its `.mo` file; the
//! calls are gettext's, so xgettext finds the messages in every language's sources.

use std::collections::HashMap;

/// One language's messages.
pub struct Catalog {
    /// Translations by msgid (`context\u{4}msgid` with a context); several for a plural.
    messages: HashMap<String, Vec<String>>,
    plural: Expr,
}

impl Catalog {
    /// No translations: English, the messages as written.
    pub fn english() -> Catalog {
        Catalog {
            messages: HashMap::new(),
            plural: Expr::Not1,
        }
    }

    /// Read a `.mo` file; None if it is not one.
    pub fn parse(data: &[u8]) -> Option<Catalog> {
        let word = |at: usize, big: bool| -> Option<usize> {
            let b: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
            Some(if big {
                u32::from_be_bytes(b)
            } else {
                u32::from_le_bytes(b)
            } as usize)
        };
        let big = match word(0, false)? {
            0x950412de => false,
            0xde120495 => true,
            _ => return None,
        };
        let (count, originals, translations) = (word(8, big)?, word(12, big)?, word(16, big)?);
        let string = |table: usize, i: usize| -> Option<&str> {
            let (len, at) = (word(table + i * 8, big)?, word(table + i * 8 + 4, big)?);
            std::str::from_utf8(data.get(at..at + len)?).ok()
        };
        let mut messages = HashMap::with_capacity(count);
        let mut plural = Expr::Not1;
        for i in 0..count {
            let (id, text) = (string(originals, i)?, string(translations, i)?);
            if id.is_empty() {
                // The header: the plural rule is all it is read for.
                let rule = text
                    .lines()
                    .find_map(|l| l.strip_prefix("Plural-Forms:"))
                    .and_then(|l| l.split_once("plural=").map(|(_, r)| r))
                    .map(|r| r.trim().trim_end_matches(';'));
                if let Some(e) = rule.and_then(Expr::parse) {
                    plural = e;
                }
                continue;
            }
            // A plural's msgid is "one\0many"; the key is its first form.
            let key = id.split('\0').next().unwrap_or(id).to_string();
            messages.insert(key, text.split('\0').map(String::from).collect());
        }
        Some(Catalog { messages, plural })
    }

    fn find(&self, key: &str) -> Option<&Vec<String>> {
        self.messages
            .get(key)
            .filter(|t| t.iter().any(|s| !s.is_empty()))
    }

    pub fn gettext(&self, msgid: &str) -> String {
        match self.find(msgid) {
            Some(t) => t[0].clone(),
            None => msgid.into(),
        }
    }

    /// The form for `n` of a counted message (English's when untranslated).
    pub fn ngettext(&self, one: &str, many: &str, n: u64) -> String {
        match self.find(one) {
            Some(t) => {
                let form = self.plural.eval(n) as usize;
                t.get(form).or(t.last()).cloned().unwrap_or_default()
            }
            None if n == 1 => one.into(),
            None => many.into(),
        }
    }

    /// A message with a context, for a short word that could translate two ways.
    pub fn pgettext(&self, context: &str, msgid: &str) -> String {
        match self.find(&format!("{context}\u{4}{msgid}")) {
            Some(t) => t[0].clone(),
            None => msgid.into(),
        }
    }
}

/// A plural rule: the C expression of a catalog's `Plural-Forms`, over `n`.
#[derive(Debug, PartialEq)]
enum Expr {
    N,
    Num(u64),
    /// English's `n != 1`, until a catalog says otherwise.
    Not1,
    Not(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl Expr {
    fn parse(s: &str) -> Option<Expr> {
        let mut p = Parser {
            s: s.as_bytes(),
            at: 0,
        };
        let e = p.ternary()?;
        p.space();
        (p.at == p.s.len()).then_some(e)
    }

    fn eval(&self, n: u64) -> u64 {
        match self {
            Expr::N => n,
            Expr::Num(x) => *x,
            Expr::Not1 => (n != 1) as u64,
            Expr::Not(e) => (e.eval(n) == 0) as u64,
            Expr::If(c, a, b) => {
                if c.eval(n) != 0 {
                    a.eval(n)
                } else {
                    b.eval(n)
                }
            }
            Expr::Bin(op, a, b) => {
                let (a, b) = (a.eval(n), b.eval(n));
                match op {
                    Op::Or => (a != 0 || b != 0) as u64,
                    Op::And => (a != 0 && b != 0) as u64,
                    Op::Eq => (a == b) as u64,
                    Op::Ne => (a != b) as u64,
                    Op::Lt => (a < b) as u64,
                    Op::Gt => (a > b) as u64,
                    Op::Le => (a <= b) as u64,
                    Op::Ge => (a >= b) as u64,
                    Op::Add => a.wrapping_add(b),
                    Op::Sub => a.wrapping_sub(b),
                    Op::Mul => a.wrapping_mul(b),
                    Op::Div => a.checked_div(b).unwrap_or(0),
                    Op::Rem => a.checked_rem(b).unwrap_or(0),
                }
            }
        }
    }
}

/// Recursive descent over C's precedence, from `?:` down to `!`, numbers, `n` and parentheses.
struct Parser<'a> {
    s: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.s.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    /// Take `tok` if it comes next (not `<` when `<=` does).
    fn eat(&mut self, tok: &str) -> bool {
        self.space();
        let rest = &self.s[self.at..];
        let longer = tok.len() == 1 && rest.get(1) == Some(&b'=') && b"<>!=".contains(&rest[0]);
        if rest.starts_with(tok.as_bytes()) && !longer {
            self.at += tok.len();
            true
        } else {
            false
        }
    }

    fn ternary(&mut self) -> Option<Expr> {
        let c = self.binary(0)?;
        if !self.eat("?") {
            return Some(c);
        }
        let a = self.ternary()?;
        self.eat(":").then_some(())?;
        let b = self.ternary()?;
        Some(Expr::If(Box::new(c), Box::new(a), Box::new(b)))
    }

    /// Binary operators by level, loosest first.
    const LEVELS: [&'static [(&'static str, Op)]; 6] = [
        &[("||", Op::Or)],
        &[("&&", Op::And)],
        &[("==", Op::Eq), ("!=", Op::Ne)],
        &[("<=", Op::Le), (">=", Op::Ge), ("<", Op::Lt), (">", Op::Gt)],
        &[("+", Op::Add), ("-", Op::Sub)],
        &[("*", Op::Mul), ("/", Op::Div), ("%", Op::Rem)],
    ];

    fn binary(&mut self, level: usize) -> Option<Expr> {
        if level == Self::LEVELS.len() {
            return self.unary();
        }
        let mut e = self.binary(level + 1)?;
        'more: loop {
            for (tok, op) in Self::LEVELS[level] {
                if self.eat(tok) {
                    let rhs = self.binary(level + 1)?;
                    e = Expr::Bin(*op, Box::new(e), Box::new(rhs));
                    continue 'more;
                }
            }
            return Some(e);
        }
    }

    fn unary(&mut self) -> Option<Expr> {
        if self.eat("!") {
            return Some(Expr::Not(Box::new(self.unary()?)));
        }
        if self.eat("(") {
            let e = self.ternary()?;
            return self.eat(")").then_some(e);
        }
        if self.eat("n") {
            return Some(Expr::N);
        }
        self.space();
        let start = self.at;
        while self.s.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        std::str::from_utf8(&self.s[start..self.at])
            .ok()?
            .parse()
            .ok()
            .map(Expr::Num)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A little-endian `.mo` file of `entries` (msgid, msgstr), as msgfmt writes them.
    fn mo(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut entries = entries.to_vec();
        entries.sort();
        let n = entries.len();
        let (originals, translations) = (28, 28 + n * 8);
        let mut strings = Vec::new();
        let mut tables: [Vec<(usize, usize)>; 2] = Default::default();
        let base = 28 + n * 16;
        for (i, table) in tables.iter_mut().enumerate() {
            for e in &entries {
                let s = if i == 0 { e.0 } else { e.1 };
                table.push((s.len(), base + strings.len()));
                strings.extend_from_slice(s.as_bytes());
                strings.push(0);
            }
        }
        let mut out = Vec::new();
        for w in [0x950412de, 0, n, originals, translations, 0, 0] {
            out.extend_from_slice(&(w as u32).to_le_bytes());
        }
        for (len, at) in tables.concat() {
            out.extend_from_slice(&(len as u32).to_le_bytes());
            out.extend_from_slice(&(at as u32).to_le_bytes());
        }
        out.extend(strings);
        out
    }

    const POLISH: &str = "Plural-Forms: nplurals=3; plural=(n==1 ? 0 : n%10>=2 && n%10<=4 && \
                          (n%100<10 || n%100>=20) ? 1 : 2);\n";

    #[test]
    fn messages_contexts_and_polish_plurals() {
        let c = Catalog::parse(&mo(&[
            ("", POLISH),
            ("Today", "Dzisiaj"),
            ("category\u{4}Other", "Inne"),
            ("{n} task\0{n} tasks", "{n} zadanie\0{n} zadania\0{n} zadań"),
            ("Untranslated", ""),
        ]))
        .unwrap();
        assert_eq!(c.gettext("Today"), "Dzisiaj");
        assert_eq!(c.gettext("Untranslated"), "Untranslated");
        assert_eq!(c.gettext("Missing"), "Missing");
        assert_eq!(c.pgettext("category", "Other"), "Inne");
        assert_eq!(c.pgettext("to plan", "Other"), "Other");
        let tasks = |n| c.ngettext("{n} task", "{n} tasks", n);
        assert_eq!(tasks(1), "{n} zadanie");
        for n in [2, 3, 4, 22, 104] {
            assert_eq!(tasks(n), "{n} zadania", "{n}");
        }
        for n in [0, 5, 11, 12, 14, 25, 112] {
            assert_eq!(tasks(n), "{n} zadań", "{n}");
        }
        assert_eq!(c.ngettext("{n} event", "{n} events", 1), "{n} event");
        assert_eq!(c.ngettext("{n} event", "{n} events", 5), "{n} events");
    }

    #[test]
    fn plural_rules_of_the_catalogs() {
        let rule = |s| Expr::parse(s).unwrap();
        assert_eq!(rule("(n != 1)").eval(1), 0);
        assert_eq!(rule("(n != 1)").eval(0), 1);
        assert_eq!(rule("(n > 1)").eval(0), 0, "French: 0 is singular");
        assert_eq!(rule("n>1").eval(2), 1);
        assert_eq!(rule("!(n % 10 == 1) || n == 11").eval(21), 0);
        assert_eq!(rule("n / 0").eval(5), 0, "no panic");
        assert!(Expr::parse("n ==").is_none());
        assert!(Expr::parse("(n").is_none());
        assert!(Catalog::parse(b"not a catalog").is_none());
    }

    /// The real catalogs, as msgfmt compiles them (when it is installed).
    #[test]
    fn the_polish_catalog() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../po");
        let out = std::env::temp_dir().join(format!("todav-pl-{}.mo", std::process::id()));
        let made = std::process::Command::new("msgfmt")
            .args(["-o", &out.to_string_lossy(), &format!("{root}/pl.po")])
            .status()
            .is_ok_and(|s| s.success());
        if !made {
            return; // no gettext tools here
        }
        let c = Catalog::parse(&std::fs::read(&out).unwrap()).unwrap();
        let _ = std::fs::remove_file(out);
        assert_eq!(c.gettext("Today"), "Dzisiaj");
        assert_eq!(c.pgettext("category", "Other"), "Inne");
        assert_eq!(c.ngettext("{n} event", "{n} events", 5), "{n} wydarzeń");
    }
}
