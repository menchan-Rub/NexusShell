/// テーブル形式でデータを表示するヘルパー
#[derive(Debug)]
pub struct TableFormatter {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl TableFormatter {
    pub fn new(headers: Vec<String>) -> Self {
        Self {
            headers,
            rows: Vec::new(),
        }
    }

    pub fn add_row(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    pub fn print(&self) {
        if self.headers.is_empty() {
            return;
        }

        // 各列の最大幅を計算
        let mut widths = self.headers.iter().map(|h| h.len()).collect::<Vec<_>>();
        
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }

        // ヘッダーを印刷
        for (i, header) in self.headers.iter().enumerate() {
            if i > 0 {
                print!("  ");
            }
            print!("{:<width$}", header, width = widths.get(i).unwrap_or(&0));
        }
        println!();

        // 行を印刷
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i > 0 {
                    print!("  ");
                }
                print!("{:<width$}", cell, width = widths.get(i).unwrap_or(&0));
            }
            println!();
        }
    }
} 