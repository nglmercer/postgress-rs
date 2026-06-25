use postgres::{Client, NoTls};

pub struct PostgresBench {
    client: Client,
}

impl PostgresBench {
    pub fn connect() -> Result<Self, postgres::Error> {
        let client = Client::connect("host=localhost user=postgres dbname=postgres", NoTls)?;
        Ok(Self { client })
    }

    pub fn setup_table(&mut self, table_name: &str, rows: usize) {
        self.client
            .execute(&format!("DROP TABLE IF EXISTS {}", table_name), &[])
            .unwrap();
        self.client
            .execute(
                &format!(
                    "CREATE TABLE {} (id SERIAL PRIMARY KEY, name TEXT, value INTEGER)",
                    table_name
                ),
                &[],
            )
            .unwrap();

        for i in 0..rows {
            self.client
                .execute(
                    &format!("INSERT INTO {} (name, value) VALUES ($1, $2)", table_name),
                    &[&format!("name_{}", i), &(i as i32)],
                )
                .unwrap();
        }
    }

    pub fn execute_query(&mut self, query: &str) -> Vec<postgres::Row> {
        self.client.query(query, &[]).unwrap()
    }

    pub fn execute_simple(&mut self, query: &str) -> u64 {
        self.client.execute(query, &[]).unwrap()
    }

    pub fn query_one(&mut self, query: &str) -> postgres::Row {
        self.client.query_one(query, &[]).unwrap()
    }
}
