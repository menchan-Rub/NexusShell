use std::fmt;
use std::ops::{Add, AddAssign, Range, Sub, SubAssign};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// ソースファイル情報を表す構造体
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// ファイルパス
    pub path: PathBuf,
    /// ファイルの内容
    pub content: Arc<String>,
    /// 各行の開始位置のキャッシュ
    line_starts: Vec<usize>,
}

impl SourceFile {
    /// 新しいソースファイルを作成
    pub fn new<P: AsRef<Path>>(path: P, content: String) -> Self {
        let content = Arc::new(content);
        let line_starts = Self::compute_line_starts(&content);
        
        Self {
            path: path.as_ref().to_path_buf(),
            content,
            line_starts,
        }
    }

    /// メモリ上のソースコードから新しいソースファイルを作成
    pub fn from_memory(content: String, name: &str) -> Self {
        let content = Arc::new(content);
        let line_starts = Self::compute_line_starts(&content);
        
        Self {
            path: PathBuf::from(name),
            content,
            line_starts,
        }
    }

    /// 各行の開始位置を計算
    fn compute_line_starts(content: &str) -> Vec<usize> {
        let mut line_starts = vec![0];
        
        for (i, c) in content.char_indices() {
            if c == '\n' {
                line_starts.push(i + 1);
            }
        }
        
        line_starts
    }

    /// バイトオフセットから行と列を取得
    pub fn location_from_offset(&self, offset: usize) -> (usize, usize) {
        let line_index = match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(insert_pos) => insert_pos.saturating_sub(1),
        };
        
        let line = line_index + 1; // 1ベースのライン番号
        let column = offset - self.line_starts[line_index] + 1; // 1ベースのカラム番号
        
        (line, column)
    }

    /// 行と列からバイトオフセットを取得
    pub fn offset_from_location(&self, line: usize, column: usize) -> Option<usize> {
        if line == 0 || line > self.line_starts.len() {
            return None;
        }
        
        let line_start = self.line_starts[line - 1];
        let line_length = if line < self.line_starts.len() {
            self.line_starts[line] - line_start
        } else {
            self.content.len() - line_start
        };
        
        if column == 0 || column > line_length {
            return None;
        }
        
        Some(line_start + column - 1)
    }

    /// 指定された行の内容を取得
    pub fn line_content(&self, line: usize) -> Option<&str> {
        if line == 0 || line > self.line_starts.len() {
            return None;
        }
        
        let start = self.line_starts[line - 1];
        let end = if line < self.line_starts.len() {
            self.line_starts[line]
        } else {
            self.content.len()
        };
        
        Some(&self.content[start..end])
    }

    /// 指定されたスパンの内容を取得
    pub fn span_content(&self, span: &Span) -> &str {
        let start = span.start.min(self.content.len());
        let end = span.end.min(self.content.len());
        &self.content[start..end]
    }

    /// ファイル名を取得
    pub fn filename(&self) -> &str {
        self.path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<unknown>")
    }

    /// 総行数を取得
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// ファイルサイズを取得
    pub fn size(&self) -> usize {
        self.content.len()
    }
}

/// ソースコード内の位置情報を表す構造体
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Span {
    /// 開始位置（バイトオフセット）
    pub start: usize,
    /// 終了位置（バイトオフセット）
    pub end: usize,
    /// 行番号（1始まり）
    pub line: usize,
    /// 列番号（1始まり）
    pub column: usize,
}

impl Span {
    /// 新しいスパンを作成
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

    /// デフォルトのスパンを作成
    pub fn default() -> Self {
        Self {
            start: 0,
            end: 0,
            line: 1,
            column: 1,
        }
    }

    /// オフセットのみからスパンを作成
    pub fn from_offsets(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            line: 1,
            column: 1,
        }
    }

    /// 行と列からスパンを作成
    pub fn from_line_column(line: usize, column: usize, length: usize) -> Self {
        Self {
            start: 0,
            end: length,
            line,
            column,
        }
    }

    /// 単一の位置のスパンを作成
    pub fn at_offset(offset: usize) -> Self {
        Self {
            start: offset,
            end: offset,
            line: 1,
            column: 1,
        }
    }

    /// ソースファイルからスパンの位置情報を更新
    pub fn with_source_info(mut self, source: &SourceFile) -> Self {
        let (line, column) = source.location_from_offset(self.start);
        self.line = line;
        self.column = column;
        self
    }

    /// スパンを結合
    pub fn merge(&self, other: &Span) -> Self {
        let start = std::cmp::min(self.start, other.start);
        let end = std::cmp::max(self.end, other.end);
        
        // 行と列は開始位置の情報を使用
        let (line, column) = if self.start <= other.start {
            (self.line, self.column)
        } else {
            (other.line, other.column)
        };

        Self {
            start,
            end,
            line,
            column,
        }
    }

    /// スパンの範囲を拡張
    pub fn expand(&self, delta: usize) -> Self {
        Self {
            start: self.start.saturating_sub(delta),
            end: self.end + delta,
            ..*self
        }
    }

    /// スパンの範囲を縮小
    pub fn shrink(&self, delta: usize) -> Self {
        let start = self.start + delta;
        let end = self.end.saturating_sub(delta);
        
        Self {
            start: start.min(end),
            end: end.max(start),
            ..*self
        }
    }

    /// 長さを取得
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// 空かどうかをチェック
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// 範囲内かどうかをチェック
    pub fn contains(&self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }

    /// 別のスパンを完全に含むかどうかをチェック
    pub fn contains_span(&self, other: &Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// 範囲が重なっているかをチェック
    pub fn overlaps(&self, other: &Span) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// スパンの始点を取得
    pub fn start_point(&self) -> (usize, usize) {
        (self.line, self.column)
    }

    /// スパンの終点を取得
    pub fn end_point(&self, source: &SourceFile) -> (usize, usize) {
        source.location_from_offset(self.end)
    }

    /// 人間が読みやすい位置情報を取得
    pub fn to_location_string(&self) -> String {
        format!("{}:{}", self.line, self.column)
    }

    /// スパンの範囲を文字列として取得
    pub fn to_range_string(&self) -> String {
        format!("{}:{}-{}", self.line, self.column, self.column + self.len())
    }

    /// 詳細な位置情報を文字列として取得
    pub fn to_detailed_string(&self, source: &SourceFile) -> String {
        let (end_line, end_column) = source.location_from_offset(self.end);
        if self.line == end_line {
            format!("{}:{}-{}", self.line, self.column, end_column)
        } else {
            format!("{}:{}-{}:{}", self.line, self.column, end_line, end_column)
        }
    }

    /// ファイル内の位置情報を含む文字列を取得
    pub fn with_file_info(&self, filename: &str) -> String {
        format!("{}:{}:{}", filename, self.line, self.column)
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}-{}", self.line, self.column, self.column + (self.end - self.start))
    }
}

impl From<Range<usize>> for Span {
    fn from(range: Range<usize>) -> Self {
        Self::from_offsets(range.start, range.end)
    }
}

impl From<Span> for Range<usize> {
    fn from(span: Span) -> Self {
        span.start..span.end
    }
}

impl Add<usize> for Span {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self {
            start: self.start + rhs,
            end: self.end + rhs,
            ..self
        }
    }
}

impl AddAssign<usize> for Span {
    fn add_assign(&mut self, rhs: usize) {
        self.start += rhs;
        self.end += rhs;
    }
}

impl Sub<usize> for Span {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        Self {
            start: self.start.saturating_sub(rhs),
            end: self.end.saturating_sub(rhs),
            ..self
        }
    }
}

impl SubAssign<usize> for Span {
    fn sub_assign(&mut self, rhs: usize) {
        self.start = self.start.saturating_sub(rhs);
        self.end = self.end.saturating_sub(rhs);
    }
}

/// ソースコードの位置情報を持つトレイト
pub trait Spanned {
    /// スパン情報を取得
    fn span(&self) -> Span;
}

/// 複数のスパンを結合するためのヘルパー関数
pub fn merge_spans<I>(spans: I) -> Option<Span>
where
    I: IntoIterator<Item = Span>,
{
    let mut iter = spans.into_iter();
    let first = iter.next()?;
    Some(iter.fold(first, |acc, span| acc.merge(&span)))
}

/// ダミーのスパンを作成するためのヘルパー関数
pub fn dummy_span() -> Span {
    Span::default()
}

/// 開始と終了のオフセットからスパンを作成するためのヘルパー関数
pub fn span_from_offsets(start: usize, end: usize) -> Span {
    Span::from_offsets(start, end)
}

/// 行と列からスパンを作成するためのヘルパー関数
pub fn span_from_line_col(line: usize, column: usize, length: usize) -> Span {
    Span::from_line_column(line, column, length)
}

/// ソースコードのスニペットを表す構造体
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSnippet {
    /// ソースファイル情報
    pub source: Arc<SourceFile>,
    /// スパン情報
    pub span: Span,
    /// コンテキスト行数
    pub context_lines: usize,
}

impl SourceSnippet {
    /// 新しいソースコードスニペットを作成
    pub fn new(source: Arc<SourceFile>, span: Span, context_lines: usize) -> Self {
        Self {
            source,
            span,
            context_lines,
        }
    }

    /// スニペットの内容を取得
    pub fn content(&self) -> &str {
        self.source.span_content(&self.span)
    }

    /// コンテキスト行を含むスニペットを生成
    pub fn with_context(&self) -> String {
        let (start_line, _) = self.source.location_from_offset(self.span.start);
        let (end_line, _) = self.source.location_from_offset(self.span.end);
        
        let start_context = start_line.saturating_sub(self.context_lines);
        let end_context = (end_line + self.context_lines).min(self.source.line_count());
        
        let mut result = String::new();
        let file_name = self.source.filename();
        
        result.push_str(&format!("// {}:{}:{}\n", file_name, start_line, self.span.column));
        
        for line_num in start_context..=end_context {
            if let Some(line) = self.source.line_content(line_num) {
                let prefix = format!("{:4} | ", line_num);
                result.push_str(&prefix);
                result.push_str(line);
                
                // 最後の行が改行で終わっていない場合、改行を追加
                if !line.ends_with('\n') {
                    result.push('\n');
                }
                
                // エラー位置を示すマーカーを追加
                if line_num >= start_line && line_num <= end_line {
                    result.push_str(&format!("     | "));
                    
                    let (line_start, line_end) = if line_num == start_line && line_num == end_line {
                        // スパンが1行に収まる場合
                        (self.span.column, self.span.column + self.span.len())
                    } else if line_num == start_line {
                        // 複数行スパンの先頭行
                        (self.span.column, line.len() + 1)
                    } else if line_num == end_line {
                        // 複数行スパンの最終行
                        (1, self.source.location_from_offset(self.span.end).1)
                    } else {
                        // 複数行スパンの中間行
                        (1, line.len() + 1)
                    };
                    
                    for i in 1..line_start {
                        result.push(' ');
                    }
                    
                    for _ in line_start..line_end {
                        result.push('^');
                    }
                    
                    result.push('\n');
                }
            }
        }
        
        result
    }

    /// スニペットの一行要約を生成
    pub fn summary(&self) -> String {
        let content = self.content();
        let max_len = 50;
        
        let summary = if content.len() > max_len {
            format!("{}...", &content[..max_len])
        } else {
            content.to_string()
        };
        
        format!(
            "{}:{}:{}: {}",
            self.source.filename(),
            self.span.line,
            self.span.column,
            summary.replace('\n', "\\n")
        )
    }
}

/// 位置情報付きの値を表す構造体
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located<T> {
    /// 値
    pub value: T,
    /// スパン情報
    pub span: Span,
}

impl<T> Located<T> {
    /// 新しい位置情報付き値を作成
    pub fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }

    /// 値を変換
    pub fn map<U, F>(self, f: F) -> Located<U>
    where
        F: FnOnce(T) -> U,
    {
        Located {
            value: f(self.value),
            span: self.span,
        }
    }

    /// 値を取得して消費
    pub fn into_value(self) -> T {
        self.value
    }

    /// 値の参照を取得
    pub fn value(&self) -> &T {
        &self.value
    }

    /// 値の可変参照を取得
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> Spanned for Located<T> {
    fn span(&self) -> Span {
        self.span
    }
}

impl<T: fmt::Display> fmt::Display for Located<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.value, self.span)
    }
}

/// 位置情報付きのスライスを表す構造体
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedSlice<'a, T> {
    /// スライス
    pub items: &'a [T],
    /// スパン情報
    pub span: Span,
}

impl<'a, T> LocatedSlice<'a, T> {
    /// 新しい位置情報付きスライスを作成
    pub fn new(items: &'a [T], span: Span) -> Self {
        Self { items, span }
    }

    /// 空かどうかをチェック
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 長さを取得
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

impl<'a, T> Spanned for LocatedSlice<'a, T> {
    fn span(&self) -> Span {
        self.span
    }
} 