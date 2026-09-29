//! The statements content files use, parsed from [`super::lex`] tokens.
//!
//! Not ACE-derived: the subset of MySQL's grammar that ACE's content writers (the per-object
//! `DELETE` + `INSERT` files), mysqldump and phpMyAdmin exports produce, plus `UPDATE`. Anything
//! else is an error naming the statement, never silently skipped; statements that cannot change
//! table contents (`USE`, `LOCK TABLES`, `SET NAMES`, transactions) are recognised and ignored.

use super::lex::{Tok, Token};

/// A scalar expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Null,
    /// A number literal, as written.
    Num(String),
    Str(Vec<u8>),
    /// `0x…` / `X'…'`: a binary string that is a number in numeric context.
    Hex(Vec<u8>),
    Bits(u64),
    Bool(bool),
    UserVar(String),
    SysVar(String),
    /// `LAST_INSERT_ID()`.
    LastInsertId,
    /// `CURRENT_TIMESTAMP`, `NOW()`, `CURRENT_TIMESTAMP()`, `UTC_TIMESTAMP()`.
    Now,
    /// `DEFAULT` in a `VALUES` list or `SET` clause.
    Default,
    Column(String),
    Neg(Box<Expr>),
    Binary(Box<Expr>, &'static str, Box<Expr>),
    /// `IF(cond, then, else)`.
    If(Box<Cond>, Box<Expr>, Box<Expr>),
    /// `IFNULL(a, b)` and `COALESCE(a, …)`: the first non-NULL.
    Coalesce(Vec<Expr>),
}

/// A `WHERE` condition.
#[derive(Debug, Clone, PartialEq)]
pub enum Cond {
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
    Not(Box<Cond>),
    Cmp(Expr, &'static str, Expr),
    In(Expr, Vec<Expr>, bool),
    IsNull(Expr, bool),
    Between(Expr, Expr, Expr, bool),
    Like(Expr, Expr, bool),
    /// A bare expression used as a condition (`WHERE 1`).
    Truth(Expr),
}

/// One column definition of a `CREATE TABLE` or `ALTER TABLE … MODIFY`.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDef {
    pub name: String,
    /// The type name, lower-cased (`int`, `varchar`, `float`, …).
    pub ty: String,
    /// The first type argument (`varchar(255)` → 255).
    pub len: Option<u32>,
    pub unsigned: bool,
    pub not_null: bool,
    pub default: Option<Expr>,
    pub auto_increment: bool,
    pub on_update_now: bool,
    /// `GENERATED ALWAYS AS (expr)`.
    pub generated: Option<Expr>,
}

/// A key or constraint of a `CREATE TABLE` or `ALTER TABLE … ADD`.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyDef {
    Primary(Vec<String>),
    Unique(String, Vec<String>),
    Index(String, Vec<String>),
    Foreign {
        name: String,
        columns: Vec<String>,
        table: String,
        ref_columns: Vec<String>,
        on_delete: RefAction,
        on_update: RefAction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefAction {
    Restrict,
    Cascade,
    SetNull,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateTable {
    pub name: String,
    pub columns: Vec<ColumnDef>,
    pub keys: Vec<KeyDef>,
    /// The `AUTO_INCREMENT=n` table option.
    pub auto_increment: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AlterOp {
    Add(KeyDef),
    Modify(ColumnDef),
    AutoIncrement(u64),
    DropKey(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SetTarget {
    User(String),
    /// A system variable, lower-cased, without `@@`, `SESSION.` or `GLOBAL.`.
    System(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Insert {
        replace: bool,
        ignore: bool,
        table: String,
        columns: Option<Vec<String>>,
        rows: Vec<Vec<Expr>>,
    },
    Delete {
        table: String,
        filter: Option<Cond>,
    },
    Update {
        table: String,
        sets: Vec<(String, Expr)>,
        filter: Option<Cond>,
    },
    Set(Vec<(SetTarget, Expr)>),
    DropTable {
        tables: Vec<String>,
        if_exists: bool,
    },
    Truncate(String),
    CreateTable(CreateTable),
    AlterTable {
        table: String,
        ops: Vec<AlterOp>,
    },
    /// A statement that cannot change table contents.
    Ignored,
}

/// One statement and the line it starts on.
#[derive(Debug, Clone, PartialEq)]
pub struct Located {
    pub line: u32,
    pub stmt: Stmt,
}

/// Split tokens at top-level `;` and parse each statement.
pub fn parse_script(tokens: &[Token]) -> Result<Vec<Located>, String> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, t) in tokens.iter().enumerate() {
        if t.kind == Tok::Punct(";") {
            if i > start {
                out.push(parse_one(&tokens[start..i])?);
            }
            start = i + 1;
        }
    }
    if start < tokens.len() {
        out.push(parse_one(&tokens[start..])?);
    }
    Ok(out)
}

fn parse_one(tokens: &[Token]) -> Result<Located, String> {
    let line = tokens[0].line;
    let mut p = P { t: tokens, i: 0 };
    let stmt = p.statement().map_err(|e| format!("line {line}: {e}"))?;
    if p.i < tokens.len() {
        return Err(format!(
            "line {line}: unexpected {:?} after the statement (line {})",
            tokens[p.i].kind, tokens[p.i].line
        ));
    }
    Ok(Located { line, stmt })
}

/// Parse a single `CREATE TABLE` statement (as mysqldump writes one).
pub fn parse_create_table(text: &[u8]) -> Result<CreateTable, String> {
    let toks = super::lex::tokenize(text)?;
    let toks: Vec<Token> = toks
        .into_iter()
        .take_while(|t| t.kind != Tok::Punct(";"))
        .collect();
    match parse_one(&toks)?.stmt {
        Stmt::CreateTable(c) => Ok(c),
        other => Err(format!("expected CREATE TABLE, found {other:?}")),
    }
}

struct P<'a> {
    t: &'a [Token],
    i: usize,
}

type R<T> = Result<T, String>;

impl P<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i).map(|t| &t.kind)
    }

    fn peek_at(&self, k: usize) -> Option<&Tok> {
        self.t.get(self.i + k).map(|t| &t.kind)
    }

    fn next(&mut self) -> R<Tok> {
        let t = self
            .t
            .get(self.i)
            .ok_or("unexpected end of statement")?
            .kind
            .clone();
        self.i += 1;
        Ok(t)
    }

    fn is_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Tok::Word(w)) if w.eq_ignore_ascii_case(kw))
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.is_kw(kw) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn kw(&mut self, kw: &str) -> R<()> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            Err(format!("expected {kw}, found {:?}", self.peek()))
        }
    }

    fn is_p(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Tok::Punct(q)) if *q == p)
    }

    fn eat_p(&mut self, p: &str) -> bool {
        if self.is_p(p) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn punct(&mut self, p: &str) -> R<()> {
        if self.eat_p(p) {
            Ok(())
        } else {
            Err(format!("expected '{p}', found {:?}", self.peek()))
        }
    }

    /// An identifier: bare or quoted. `db.table` keeps the last part.
    fn ident(&mut self) -> R<String> {
        let mut name = match self.next()? {
            Tok::Word(w) | Tok::Quoted(w) => w,
            other => return Err(format!("expected a name, found {other:?}")),
        };
        while self.is_p(".") && matches!(self.peek_at(1), Some(Tok::Word(_) | Tok::Quoted(_))) {
            self.i += 1;
            name = self.ident()?;
        }
        Ok(name)
    }

    fn ident_list(&mut self) -> R<Vec<String>> {
        self.punct("(")?;
        let mut v = Vec::new();
        loop {
            v.push(self.ident()?);
            // `name`(100): an index prefix length.
            if self.eat_p("(") {
                self.next()?;
                self.punct(")")?;
            }
            // ASC / DESC in an index definition.
            let _ = self.eat_kw("ASC") || self.eat_kw("DESC");
            if !self.eat_p(",") {
                break;
            }
        }
        self.punct(")")?;
        Ok(v)
    }

    fn skip_to_end(&mut self) {
        self.i = self.t.len();
    }

    fn statement(&mut self) -> R<Stmt> {
        let Some(first) = self.peek().and_then(Tok::keyword) else {
            return Err(format!("a statement cannot start with {:?}", self.peek()));
        };
        match first.as_str() {
            "INSERT" | "REPLACE" => self.insert(),
            "DELETE" => self.delete(),
            "UPDATE" => self.update(),
            "SET" => self.set(),
            "DROP" => self.drop(),
            "TRUNCATE" => {
                self.i += 1;
                self.eat_kw("TABLE");
                Ok(Stmt::Truncate(self.ident()?))
            }
            "CREATE" => self.create(),
            "ALTER" => self.alter(),
            "USE" | "LOCK" | "UNLOCK" | "START" | "BEGIN" | "COMMIT" => {
                self.skip_to_end();
                Ok(Stmt::Ignored)
            }
            other => Err(format!("unsupported statement {other}")),
        }
    }

    fn insert(&mut self) -> R<Stmt> {
        let replace = self.next()?.keyword().as_deref() == Some("REPLACE");
        let _ =
            self.eat_kw("LOW_PRIORITY") || self.eat_kw("DELAYED") || self.eat_kw("HIGH_PRIORITY");
        let ignore = self.eat_kw("IGNORE");
        self.eat_kw("INTO");
        let table = self.ident()?;
        let columns = if self.is_p("(") {
            Some(self.ident_list()?)
        } else {
            None
        };
        if !(self.eat_kw("VALUES") || self.eat_kw("VALUE")) {
            return Err(format!(
                "INSERT INTO `{table}`: only VALUES lists are supported"
            ));
        }
        let mut rows = Vec::new();
        loop {
            self.punct("(")?;
            let mut row = Vec::new();
            if !self.is_p(")") {
                loop {
                    row.push(self.expr()?);
                    if !self.eat_p(",") {
                        break;
                    }
                }
            }
            self.punct(")")?;
            rows.push(row);
            if !self.eat_p(",") {
                break;
            }
        }
        if self.is_kw("ON") {
            return Err(format!(
                "INSERT INTO `{table}`: ON DUPLICATE KEY UPDATE is not supported"
            ));
        }
        Ok(Stmt::Insert {
            replace,
            ignore,
            table,
            columns,
            rows,
        })
    }

    fn delete(&mut self) -> R<Stmt> {
        self.kw("DELETE")?;
        let _ = self.eat_kw("LOW_PRIORITY") || self.eat_kw("QUICK") || self.eat_kw("IGNORE");
        self.kw("FROM")?;
        let table = self.ident()?;
        let filter = if self.eat_kw("WHERE") {
            Some(self.cond()?)
        } else {
            None
        };
        if self.is_kw("ORDER") || self.is_kw("LIMIT") {
            return Err(format!(
                "DELETE FROM `{table}`: ORDER BY / LIMIT are not supported"
            ));
        }
        Ok(Stmt::Delete { table, filter })
    }

    fn update(&mut self) -> R<Stmt> {
        self.kw("UPDATE")?;
        let _ = self.eat_kw("LOW_PRIORITY") || self.eat_kw("IGNORE");
        let table = self.ident()?;
        self.kw("SET")?;
        let mut sets = Vec::new();
        loop {
            let col = self.ident()?;
            self.punct("=")?;
            sets.push((col, self.expr()?));
            if !self.eat_p(",") {
                break;
            }
        }
        let filter = if self.eat_kw("WHERE") {
            Some(self.cond()?)
        } else {
            None
        };
        if self.is_kw("ORDER") || self.is_kw("LIMIT") {
            return Err(format!(
                "UPDATE `{table}`: ORDER BY / LIMIT are not supported"
            ));
        }
        Ok(Stmt::Update {
            table,
            sets,
            filter,
        })
    }

    fn set(&mut self) -> R<Stmt> {
        self.kw("SET")?;
        if self.is_kw("NAMES") || self.is_kw("CHARACTER") || self.is_kw("CHARSET") {
            self.skip_to_end();
            return Ok(Stmt::Ignored);
        }
        let mut v = Vec::new();
        loop {
            let target = match self.next()? {
                Tok::UserVar(n) => SetTarget::User(n),
                Tok::SysVar(n) => SetTarget::System(sys_name(&n)),
                Tok::Word(w) => {
                    let w = w.to_ascii_lowercase();
                    if matches!(w.as_str(), "session" | "global" | "local") {
                        SetTarget::System(self.ident()?.to_ascii_lowercase())
                    } else {
                        SetTarget::System(w)
                    }
                }
                other => return Err(format!("SET: unexpected {other:?}")),
            };
            if !self.eat_p("=") {
                return Err("SET: expected '='".into());
            }
            v.push((target, self.expr()?));
            if !self.eat_p(",") {
                break;
            }
        }
        Ok(Stmt::Set(v))
    }

    fn drop(&mut self) -> R<Stmt> {
        self.kw("DROP")?;
        self.eat_kw("TEMPORARY");
        if !self.eat_kw("TABLE") {
            return Err("only DROP TABLE is supported".into());
        }
        let if_exists = if self.eat_kw("IF") {
            self.kw("EXISTS")?;
            true
        } else {
            false
        };
        let mut tables = Vec::new();
        loop {
            tables.push(self.ident()?);
            if !self.eat_p(",") {
                break;
            }
        }
        let _ = self.eat_kw("RESTRICT") || self.eat_kw("CASCADE");
        Ok(Stmt::DropTable { tables, if_exists })
    }

    fn create(&mut self) -> R<Stmt> {
        self.kw("CREATE")?;
        if self.is_kw("DATABASE") || self.is_kw("SCHEMA") {
            self.skip_to_end();
            return Ok(Stmt::Ignored);
        }
        if !self.eat_kw("TABLE") {
            return Err("only CREATE TABLE is supported".into());
        }
        if self.eat_kw("IF") {
            self.kw("NOT")?;
            self.kw("EXISTS")?;
            return Err("CREATE TABLE IF NOT EXISTS is not supported".into());
        }
        let name = self.ident()?;
        self.punct("(")?;
        let mut columns = Vec::new();
        let mut keys = Vec::new();
        loop {
            if let Some(k) = self.key_def()? {
                keys.push(k);
            } else {
                let c = self.column_def()?;
                if self.is_kw("PRIMARY") {
                    // `id int … PRIMARY KEY` inline.
                    self.i += 1;
                    self.kw("KEY")?;
                    keys.push(KeyDef::Primary(vec![c.name.clone()]));
                }
                columns.push(c);
            }
            if !self.eat_p(",") {
                break;
            }
        }
        self.punct(")")?;
        // Table options: ENGINE=…, AUTO_INCREMENT=n, DEFAULT CHARSET=…, COMMENT='…'.
        let mut auto_increment = None;
        while self.i < self.t.len() {
            if self.eat_kw("AUTO_INCREMENT") {
                self.eat_p("=");
                auto_increment = Some(self.u64()?);
            } else {
                self.i += 1;
            }
        }
        Ok(Stmt::CreateTable(CreateTable {
            name,
            columns,
            keys,
            auto_increment,
        }))
    }

    fn u64(&mut self) -> R<u64> {
        match self.next()? {
            Tok::Num(n) => n
                .parse()
                .map_err(|_| format!("expected an integer, found {n}")),
            other => Err(format!("expected an integer, found {other:?}")),
        }
    }

    /// A key or constraint definition, or `None` if the next item is a column.
    fn key_def(&mut self) -> R<Option<KeyDef>> {
        let Some(k) = self.peek().and_then(Tok::keyword) else {
            return Ok(None);
        };
        let name_opt = |p: &mut Self| -> R<String> {
            if p.is_p("(") {
                Ok(String::new())
            } else {
                p.ident()
            }
        };
        match k.as_str() {
            "PRIMARY" => {
                self.i += 1;
                self.kw("KEY")?;
                let cols = self.ident_list()?;
                self.index_options();
                Ok(Some(KeyDef::Primary(cols)))
            }
            "UNIQUE" => {
                self.i += 1;
                let _ = self.eat_kw("KEY") || self.eat_kw("INDEX");
                let name = name_opt(self)?;
                let cols = self.ident_list()?;
                self.index_options();
                Ok(Some(KeyDef::Unique(name, cols)))
            }
            "KEY" | "INDEX" => {
                self.i += 1;
                let name = name_opt(self)?;
                let cols = self.ident_list()?;
                self.index_options();
                Ok(Some(KeyDef::Index(name, cols)))
            }
            "FULLTEXT" | "SPATIAL" => {
                self.i += 1;
                let _ = self.eat_kw("KEY") || self.eat_kw("INDEX");
                let name = name_opt(self)?;
                let cols = self.ident_list()?;
                Ok(Some(KeyDef::Index(name, cols)))
            }
            "CONSTRAINT" | "FOREIGN" => {
                let mut name = String::new();
                if self.eat_kw("CONSTRAINT")
                    && !self.is_kw("FOREIGN")
                    && !self.is_kw("PRIMARY")
                    && !self.is_kw("UNIQUE")
                {
                    name = self.ident()?;
                }
                if self.is_kw("PRIMARY") || self.is_kw("UNIQUE") {
                    return self.key_def();
                }
                self.kw("FOREIGN")?;
                self.kw("KEY")?;
                if !self.is_p("(") {
                    self.ident()?;
                }
                let columns = self.ident_list()?;
                self.kw("REFERENCES")?;
                let table = self.ident()?;
                let ref_columns = self.ident_list()?;
                let mut on_delete = RefAction::Restrict;
                let mut on_update = RefAction::Restrict;
                while self.eat_kw("ON") {
                    let which_delete = if self.eat_kw("DELETE") {
                        true
                    } else {
                        self.kw("UPDATE")?;
                        false
                    };
                    let action = if self.eat_kw("CASCADE") {
                        RefAction::Cascade
                    } else if self.eat_kw("SET") {
                        self.kw("NULL")?;
                        RefAction::SetNull
                    } else if self.eat_kw("NO") {
                        self.kw("ACTION")?;
                        RefAction::Restrict
                    } else {
                        self.kw("RESTRICT")?;
                        RefAction::Restrict
                    };
                    if which_delete {
                        on_delete = action;
                    } else {
                        on_update = action;
                    }
                }
                Ok(Some(KeyDef::Foreign {
                    name,
                    columns,
                    table,
                    ref_columns,
                    on_delete,
                    on_update,
                }))
            }
            _ => Ok(None),
        }
    }

    fn index_options(&mut self) {
        loop {
            if self.eat_kw("USING") || self.eat_kw("COMMENT") || self.eat_kw("KEY_BLOCK_SIZE") {
                self.eat_p("=");
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn column_def(&mut self) -> R<ColumnDef> {
        let name = self.ident()?;
        let ty = match self.next()? {
            Tok::Word(w) => w.to_ascii_lowercase(),
            other => return Err(format!("column `{name}`: expected a type, found {other:?}")),
        };
        let mut len = None;
        if self.eat_p("(") {
            if let Some(Tok::Num(n)) = self.peek() {
                len = n.parse().ok();
            }
            while !self.eat_p(")") {
                self.next()?;
            }
        }
        let mut c = ColumnDef {
            name,
            ty,
            len,
            unsigned: false,
            not_null: false,
            default: None,
            auto_increment: false,
            on_update_now: false,
            generated: None,
        };
        loop {
            if self.is_p(",") || self.is_p(")") || self.is_kw("PRIMARY") || self.i >= self.t.len() {
                break;
            }
            let Some(k) = self.peek().and_then(Tok::keyword) else {
                return Err(format!("column `{}`: unexpected {:?}", c.name, self.peek()));
            };
            self.i += 1;
            match k.as_str() {
                "UNSIGNED" => c.unsigned = true,
                "SIGNED" | "ZEROFILL" | "VIRTUAL" | "STORED" | "PERSISTENT" | "BINARY"
                | "UNIQUE" => {}
                "NOT" => {
                    self.kw("NULL")?;
                    c.not_null = true;
                }
                "NULL" => c.not_null = false,
                "DEFAULT" => c.default = Some(self.unary()?),
                "AUTO_INCREMENT" => c.auto_increment = true,
                "ON" => {
                    self.kw("UPDATE")?;
                    match self.primary()? {
                        Expr::Now => c.on_update_now = true,
                        other => return Err(format!("ON UPDATE {other:?} is not supported")),
                    }
                }
                "COMMENT" => {
                    self.next()?;
                }
                "CHARACTER" => {
                    self.kw("SET")?;
                    self.next()?;
                }
                "CHARSET" | "COLLATE" => {
                    self.next()?;
                }
                "GENERATED" => {
                    self.kw("ALWAYS")?;
                    self.kw("AS")?;
                    self.punct("(")?;
                    let e = self.expr()?;
                    self.punct(")")?;
                    c.generated = Some(e);
                }
                "AS" => {
                    self.punct("(")?;
                    let e = self.expr()?;
                    self.punct(")")?;
                    c.generated = Some(e);
                }
                other => {
                    return Err(format!(
                        "column `{}`: unsupported attribute {other}",
                        c.name
                    ))
                }
            }
        }
        Ok(c)
    }

    fn alter(&mut self) -> R<Stmt> {
        self.kw("ALTER")?;
        self.kw("TABLE")?;
        let table = self.ident()?;
        let mut ops = Vec::new();
        loop {
            if self.eat_kw("ADD") {
                match self.key_def()? {
                    Some(k) => ops.push(AlterOp::Add(k)),
                    None => {
                        return Err(format!(
                            "ALTER TABLE `{table}` ADD: only keys are supported"
                        ))
                    }
                }
            } else if self.eat_kw("MODIFY") || self.eat_kw("CHANGE") {
                self.eat_kw("COLUMN");
                ops.push(AlterOp::Modify(self.column_def()?));
            } else if self.eat_kw("AUTO_INCREMENT") {
                self.eat_p("=");
                ops.push(AlterOp::AutoIncrement(self.u64()?));
            } else if self.eat_kw("DROP") {
                if self.eat_kw("PRIMARY") {
                    self.kw("KEY")?;
                    ops.push(AlterOp::DropKey("PRIMARY".into()));
                } else {
                    let _ = self.eat_kw("KEY") || self.eat_kw("INDEX");
                    ops.push(AlterOp::DropKey(self.ident()?));
                }
            } else if self.is_kw("DISABLE") || self.is_kw("ENABLE") {
                self.i += 1;
                self.kw("KEYS")?;
            } else {
                return Err(format!(
                    "ALTER TABLE `{table}`: unsupported {:?}",
                    self.peek()
                ));
            }
            if !self.eat_p(",") {
                break;
            }
        }
        Ok(Stmt::AlterTable { table, ops })
    }

    // Conditions: OR < AND < NOT < predicate.
    fn cond(&mut self) -> R<Cond> {
        let mut l = self.cond_and()?;
        while self.eat_kw("OR") || self.eat_p("||") {
            let r = self.cond_and()?;
            l = Cond::Or(Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn cond_and(&mut self) -> R<Cond> {
        let mut l = self.cond_not()?;
        while self.eat_kw("AND") || self.eat_p("&&") {
            let r = self.cond_not()?;
            l = Cond::And(Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn cond_not(&mut self) -> R<Cond> {
        if self.eat_kw("NOT") || self.eat_p("!") {
            return Ok(Cond::Not(Box::new(self.cond_not()?)));
        }
        self.predicate()
    }

    fn predicate(&mut self) -> R<Cond> {
        // A parenthesised condition, unless it is a parenthesised expression followed by an
        // operator; try the condition first and backtrack.
        if self.is_p("(") {
            let save = self.i;
            self.i += 1;
            if let Ok(c) = self.cond() {
                if self.eat_p(")") && !self.at_operator() {
                    return Ok(c);
                }
            }
            self.i = save;
        }
        let l = self.expr()?;
        let negated = self.eat_kw("NOT");
        if self.eat_kw("IN") {
            self.punct("(")?;
            let mut v = Vec::new();
            loop {
                v.push(self.expr()?);
                if !self.eat_p(",") {
                    break;
                }
            }
            self.punct(")")?;
            return Ok(Cond::In(l, v, negated));
        }
        if self.eat_kw("BETWEEN") {
            let lo = self.expr()?;
            self.kw("AND")?;
            let hi = self.expr()?;
            return Ok(Cond::Between(l, lo, hi, negated));
        }
        if self.eat_kw("LIKE") {
            let r = self.expr()?;
            return Ok(Cond::Like(l, r, negated));
        }
        if negated {
            return Err("expected IN, BETWEEN or LIKE after NOT".into());
        }
        if self.eat_kw("IS") {
            let not = self.eat_kw("NOT");
            self.kw("NULL")?;
            return Ok(Cond::IsNull(l, !not));
        }
        for op in ["=", "<=>", "<>", "!=", "<=", ">=", "<", ">"] {
            if self.eat_p(op) {
                let r = self.expr()?;
                let op: &'static str = match op {
                    "=" => "=",
                    "<=>" => "<=>",
                    "<>" | "!=" => "<>",
                    "<=" => "<=",
                    ">=" => ">=",
                    "<" => "<",
                    _ => ">",
                };
                return Ok(Cond::Cmp(l, op, r));
            }
        }
        Ok(Cond::Truth(l))
    }

    fn at_operator(&self) -> bool {
        matches!(
            self.peek(),
            Some(Tok::Punct(
                "=" | "<=>"
                    | "<>"
                    | "!="
                    | "<="
                    | ">="
                    | "<"
                    | ">"
                    | "+"
                    | "-"
                    | "*"
                    | "/"
                    | ">>"
                    | "<<"
                    | "|"
                    | "&"
            ))
        ) || self.is_kw("IN")
            || self.is_kw("IS")
            || self.is_kw("BETWEEN")
            || self.is_kw("LIKE")
            || self.is_kw("NOT")
    }

    // Expressions: | & < << >> < + - < * / < unary.
    fn expr(&mut self) -> R<Expr> {
        let mut l = self.expr_bitand()?;
        while self.eat_p("|") {
            let r = self.expr_bitand()?;
            l = Expr::Binary(Box::new(l), "|", Box::new(r));
        }
        Ok(l)
    }

    fn expr_bitand(&mut self) -> R<Expr> {
        let mut l = self.expr_shift()?;
        while self.eat_p("&") {
            let r = self.expr_shift()?;
            l = Expr::Binary(Box::new(l), "&", Box::new(r));
        }
        Ok(l)
    }

    fn expr_shift(&mut self) -> R<Expr> {
        let mut l = self.expr_add()?;
        loop {
            let op = if self.eat_p(">>") {
                ">>"
            } else if self.eat_p("<<") {
                "<<"
            } else {
                break;
            };
            let r = self.expr_add()?;
            l = Expr::Binary(Box::new(l), op, Box::new(r));
        }
        Ok(l)
    }

    fn expr_add(&mut self) -> R<Expr> {
        let mut l = self.expr_mul()?;
        loop {
            let op = if self.eat_p("+") {
                "+"
            } else if self.eat_p("-") {
                "-"
            } else {
                break;
            };
            let r = self.expr_mul()?;
            l = Expr::Binary(Box::new(l), op, Box::new(r));
        }
        Ok(l)
    }

    fn expr_mul(&mut self) -> R<Expr> {
        let mut l = self.unary()?;
        loop {
            let op = if self.eat_p("*") {
                "*"
            } else if self.eat_p("/") {
                "/"
            } else {
                break;
            };
            let r = self.unary()?;
            l = Expr::Binary(Box::new(l), op, Box::new(r));
        }
        Ok(l)
    }

    fn unary(&mut self) -> R<Expr> {
        if self.eat_p("-") {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.eat_p("+") {
            return self.unary();
        }
        self.primary()
    }

    fn primary(&mut self) -> R<Expr> {
        match self.next()? {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Hex(h) => Ok(Expr::Hex(h)),
            Tok::Bits(b) => Ok(Expr::Bits(b)),
            Tok::UserVar(v) => Ok(Expr::UserVar(v)),
            Tok::SysVar(v) => Ok(Expr::SysVar(sys_name(&v))),
            Tok::Quoted(c) => Ok(Expr::Column(c)),
            Tok::Punct("(") => {
                let e = self.expr()?;
                self.punct(")")?;
                Ok(e)
            }
            Tok::Word(w) => {
                let k = w.to_ascii_uppercase();
                let call = self.is_p("(");
                match k.as_str() {
                    "NULL" => Ok(Expr::Null),
                    "TRUE" => Ok(Expr::Bool(true)),
                    "FALSE" => Ok(Expr::Bool(false)),
                    "DEFAULT" if !call => Ok(Expr::Default),
                    "LAST_INSERT_ID" if call => {
                        self.punct("(")?;
                        self.punct(")")?;
                        Ok(Expr::LastInsertId)
                    }
                    "CURRENT_TIMESTAMP" | "NOW" | "LOCALTIMESTAMP" | "LOCALTIME"
                    | "UTC_TIMESTAMP" => {
                        if self.eat_p("(") {
                            self.punct(")")?;
                        }
                        Ok(Expr::Now)
                    }
                    "IF" if call => {
                        self.punct("(")?;
                        let c = self.cond()?;
                        self.punct(",")?;
                        let a = self.expr()?;
                        self.punct(",")?;
                        let b = self.expr()?;
                        self.punct(")")?;
                        Ok(Expr::If(Box::new(c), Box::new(a), Box::new(b)))
                    }
                    "IFNULL" | "COALESCE" if call => {
                        self.punct("(")?;
                        let mut v = Vec::new();
                        loop {
                            v.push(self.expr()?);
                            if !self.eat_p(",") {
                                break;
                            }
                        }
                        self.punct(")")?;
                        if k == "IFNULL" && v.len() != 2 {
                            return Err("IFNULL takes two arguments".into());
                        }
                        Ok(Expr::Coalesce(v))
                    }
                    _ if call => Err(format!("function {w}() is not supported")),
                    _ => Ok(Expr::Column(w)),
                }
            }
            other => Err(format!("expected a value, found {other:?}")),
        }
    }
}

/// `SESSION.sql_mode` → `sql_mode`.
fn sys_name(n: &str) -> String {
    let l = n.to_ascii_lowercase();
    for p in ["session.", "global.", "local."] {
        if let Some(rest) = l.strip_prefix(p) {
            return rest.to_owned();
        }
    }
    l
}
