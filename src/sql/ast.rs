#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Select(SelectStatement),
    Insert(InsertStatement),
    Update(UpdateStatement),
    Delete(DeleteStatement),
    CreateTable(CreateTableStatement),
    CreateIndex(CreateIndexStatement),
    CreateView(CreateViewStatement),
    AlterTable(AlterTableStatement),
    DropTable(DropTableStatement),
    DropIndex(DropIndexStatement),
    Begin(BeginStatement),
    Commit,
    Rollback,
    Explain(Box<Statement>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SetOperator {
    Union,
    UnionAll,
    Intersect,
    IntersectAll,
    Except,
    ExceptAll,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectStatement {
    pub with: Option<WithClause>,
    pub distinct: DistinctClause,
    pub select_list: Vec<SelectItem>,
    pub from: Option<FromClause>,
    pub where_clause: Option<Box<Expr>>,
    pub group_by: Vec<Expr>,
    pub having: Option<Box<Expr>>,
    pub order_by: Vec<OrderByItem>,
    pub limit: Option<LimitClause>,
    /// Set operations (UNION, INTERSECT, EXCEPT)
    pub set_operations: Vec<SetOperation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetOperation {
    pub operator: SetOperator,
    pub select: Box<SelectStatement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InsertStatement {
    pub table: ObjectName,
    pub columns: Option<Vec<String>>,
    pub source: InsertSource,
    pub on_conflict: Option<OnConflict>,
    pub returning: Vec<SelectItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OnConflict {
    DoNothing,
    DoUpdate {
        target_columns: Option<Vec<String>>,
        where_clause: Option<Box<Expr>>,
        set_clauses: Vec<SetClause>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum InsertSource {
    Values(Vec<Vec<Expr>>),
    Select(Box<SelectStatement>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateStatement {
    pub table: ObjectName,
    pub from: Option<FromClause>,
    pub set_clauses: Vec<SetClause>,
    pub where_clause: Option<Box<Expr>>,
    pub returning: Vec<SelectItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetClause {
    pub column: String,
    pub value: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeleteStatement {
    pub table: ObjectName,
    pub using: Option<FromClause>,
    pub where_clause: Option<Box<Expr>>,
    pub returning: Vec<SelectItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateTableStatement {
    pub table: ObjectName,
    pub columns: Vec<ColumnDef>,
    pub constraints: Vec<TableConstraint>,
    pub if_not_exists: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: DataType,
    pub constraints: Vec<ColumnConstraint>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ColumnConstraint {
    NotNull,
    Null,
    Default(Expr),
    PrimaryKey,
    Unique,
    References {
        table: ObjectName,
        column: String,
        on_delete: Option<ReferentialAction>,
        on_update: Option<ReferentialAction>,
    },
    Check(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReferentialAction {
    Restrict,
    Cascade,
    SetNull,
    SetDefault,
    NoAction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableConstraint {
    PrimaryKey(Vec<String>),
    Unique(Vec<String>),
    ForeignKey {
        columns: Vec<String>,
        ref_table: ObjectName,
        ref_columns: Vec<String>,
        on_delete: Option<ReferentialAction>,
        on_update: Option<ReferentialAction>,
    },
    Check(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateIndexStatement {
    pub name: ObjectName,
    pub table: ObjectName,
    pub columns: Vec<IndexColumn>,
    pub unique: bool,
    pub if_not_exists: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexColumn {
    pub expr: Expr,
    pub direction: SortDirection,
    pub nulls: NullsOrder,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateViewStatement {
    pub name: ObjectName,
    pub columns: Option<Vec<String>>,
    pub query: Box<SelectStatement>,
    pub or_replace: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlterTableStatement {
    pub table: ObjectName,
    pub action: AlterTableAction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AlterTableAction {
    AddColumn(ColumnDef),
    DropColumn {
        name: String,
        if_exists: bool,
        cascade: bool,
    },
    RenameColumn {
        old_name: String,
        new_name: String,
    },
    AlterColumn {
        name: String,
        action: AlterColumnAction,
    },
    RenameTable {
        new_name: ObjectName,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AlterColumnAction {
    SetDataType(DataType),
    SetDefault(Expr),
    DropDefault,
    SetNotNull,
    DropNotNull,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DropTableStatement {
    pub table: ObjectName,
    pub if_exists: bool,
    pub cascade: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DropIndexStatement {
    pub name: ObjectName,
    pub if_exists: bool,
    pub cascade: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BeginStatement {
    pub isolation_level: Option<IsolationLevel>,
    pub read_only: bool,
    pub deferrable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IsolationLevel {
    Serializable,
    RepeatableRead,
    ReadCommitted,
    ReadUncommitted,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DistinctClause {
    All,
    Distinct,
    DistinctOn(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct WithClause {
    pub recursive: bool,
    pub ctes: Vec<CommonTableExpr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommonTableExpr {
    pub name: String,
    pub columns: Option<Vec<String>>,
    pub materialized: Option<bool>,
    pub query: Statement,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectItem {
    Expr(Expr),
    ExprAs { expr: Expr, alias: String },
    Star,
    TableStar { table: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FromClause {
    pub joins: Vec<Join>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableRef {
    Table(ObjectName),
    Subquery(Box<SelectStatement>),
    Function(FunctionCall),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    pub table: TableRef,
    pub alias: Option<String>,
    pub join_type: JoinType,
    pub constraint: JoinConstraint,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    Cross,
    Lateral,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JoinConstraint {
    On(Box<Expr>),
    Using(Vec<String>),
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderByItem {
    pub expr: Expr,
    pub direction: SortDirection,
    pub nulls: NullsOrder,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SortDirection {
    Asc,
    Desc,
    Default,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NullsOrder {
    First,
    Last,
    Default,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LimitClause {
    All,
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectName {
    pub parts: Vec<String>,
}

impl ObjectName {
    pub fn new(parts: Vec<String>) -> Self {
        Self { parts }
    }

    pub fn single(name: String) -> Self {
        Self { parts: vec![name] }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Int,
    BigInt,
    SmallInt,
    Float,
    Double,
    Numeric(u32, u32),
    Varchar(u32),
    Char(u32),
    Text,
    Boolean,
    Date,
    Time,
    TimeTz,
    Timestamp,
    TimestampTz,
    Interval,
    Json,
    JsonB,
    Uuid,
    Serial,
    BigSerial,
    SmallSerial,
    Money,
    Inet,
    Cidr,
    MacAddr,
    Bit(u32),
    BitVarying(u32),
    TsVector,
    TsQuery,
    Array(Box<DataType>),
    Custom(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCall {
    pub name: ObjectName,
    pub args: Vec<FunctionArg>,
    pub filter: Option<Box<Expr>>,
    pub over: Option<Box<WindowSpec>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FunctionArg {
    Expr(Expr),
    Star,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowSpec {
    pub partition_by: Vec<Expr>,
    pub order_by: Vec<OrderByItem>,
    pub frame: Option<Box<FrameClause>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameClause {
    pub start: Box<FrameBound>,
    pub end: Option<Box<FrameBound>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FrameBound {
    CurrentRow,
    Preceding(Box<Expr>),
    Following(Box<Expr>),
    UnboundedPreceding,
    UnboundedFollowing,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Identifier(String),
    QualifiedIdentifier { table: String, column: String },
    Literal(Literal),
    Parameter(u32),
    IsNull(Box<Expr>),
    IsNotNull(Box<Expr>),
    InList {
        expr: Box<Expr>,
        negated: bool,
        list: Vec<Expr>,
    },
    InSubquery {
        expr: Box<Expr>,
        negated: bool,
        subquery: Box<SelectStatement>,
    },
    Between {
        expr: Box<Expr>,
        negated: bool,
        low: Box<Expr>,
        high: Box<Expr>,
    },
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOperator,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOperator,
        expr: Box<Expr>,
    },
    NestedSelect(Box<SelectStatement>),
    Function(Box<FunctionCall>),
    Array(Vec<Expr>),
    Row(Vec<Expr>),
    TypeCast {
        expr: Box<Expr>,
        data_type: DataType,
    },
    Case {
        operand: Option<Box<Expr>>,
        when_clauses: Vec<WhenClause>,
        else_clause: Option<Box<Expr>>,
    },
    Collate {
        expr: Box<Expr>,
        collation: ObjectName,
    },
    AnyComparison {
        op: BinaryOperator,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    SomeComparison {
        op: BinaryOperator,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    AtTimeZone {
        expr: Box<Expr>,
        zone: Box<Expr>,
    },
    IntervalExpr {
        value: Box<Expr>,
    },
    Extract {
        field: DatePart,
        from: Box<Expr>,
    },
    DateTrunc {
        field: DatePart,
        source: Box<Expr>,
        zone: Option<Box<Expr>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhenClause {
    pub when: Box<Expr>,
    pub then: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Number(String),
    String(String),
    Bool(bool),
    Null,
    Blob(Vec<u8>),
    Date(String),
    Time(String),
    Timestamp(String),
    TimestampTz(String),
    Interval(String),
    Json(String),
    JsonB(String),
    Uuid(String),
    Money(String),
    Bit(String),
    Hex(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum DatePart {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
    Millisecond,
    Microsecond,
    Dow,
    Doy,
    IsoDow,
    Week,
    Quarter,
    Epoch,
    IsoYear,
    Timezone,
    TimezoneHour,
    TimezoneMinute,
}

impl DatePart {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "YEAR" | "YEARS" => Some(DatePart::Year),
            "MONTH" | "MONTHS" => Some(DatePart::Month),
            "DAY" | "DAYS" => Some(DatePart::Day),
            "HOUR" | "HOURS" => Some(DatePart::Hour),
            "MINUTE" | "MINUTES" => Some(DatePart::Minute),
            "SECOND" | "SECONDS" => Some(DatePart::Second),
            "MILLISECOND" | "MILLISECONDS" => Some(DatePart::Millisecond),
            "MICROSECOND" | "MICROSECONDS" => Some(DatePart::Microsecond),
            "DOW" => Some(DatePart::Dow),
            "DOY" => Some(DatePart::Doy),
            "ISODOW" => Some(DatePart::IsoDow),
            "WEEK" | "WEEKS" => Some(DatePart::Week),
            "QUARTER" | "QUARTERS" => Some(DatePart::Quarter),
            "EPOCH" => Some(DatePart::Epoch),
            "ISOYEAR" => Some(DatePart::IsoYear),
            "TIMEZONE" => Some(DatePart::Timezone),
            "TIMEZONE_HOUR" => Some(DatePart::TimezoneHour),
            "TIMEZONE_MINUTE" => Some(DatePart::TimezoneMinute),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    Equals,
    NotEquals,
    LessThan,
    GreaterThan,
    LessOrEqual,
    GreaterOrEqual,
    And,
    Or,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    BitwiseShiftLeft,
    BitwiseShiftRight,
    Like,
    ILike,
    SimilarTo,
    Regex,
    RegexCaseInsensitive,
    NotRegex,
    NotRegexCaseInsensitive,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperator {
    Plus,
    Minus,
    Not,
    BitwiseNot,
}
