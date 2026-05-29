use boltffi_ast::PackageInfo;
use boltffi_binding::{Decl, Native, RecordDecl, lower};
use boltffi_scan::scan_source;

const SOURCE: &str = "
    #[data]
    #[repr(C)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    #[data(impl)]
    impl Point {
        pub fn origin() -> Self {
            Self { x: 0.0, y: 0.0 }
        }

        pub fn distance(&self, other: Point) -> f64 {
            let dx = self.x - other.x;
            let dy = self.y - other.y;
            (dx * dx + dy * dy).sqrt()
        }
    }

    #[export]
    pub fn make_handler() -> impl Fn(u32) -> u32 {
        |value| value
    }
";

fn record_method_counts(record: &RecordDecl<Native>) -> (usize, usize) {
    match record {
        RecordDecl::Direct(direct) => (direct.initializers().len(), direct.methods().len()),
        RecordDecl::Encoded(encoded) => (encoded.initializers().len(), encoded.methods().len()),
        _ => panic!("unexpected RecordDecl variant"),
    }
}

#[test]
fn scans_and_lowers_point_contract_to_bindings() {
    let path = std::env::temp_dir().join("boltffi_scan_to_bindings_point.rs");
    std::fs::write(&path, SOURCE).expect("write source fixture");
    let contract = scan_source(&path, PackageInfo::new("demo", None)).expect("scan");
    std::fs::remove_file(&path).ok();
    let bindings = lower::<Native>(&contract).expect("lower");

    let records = bindings
        .decls()
        .iter()
        .filter(|decl| matches!(decl, Decl::Record(_)))
        .count();
    let functions = bindings
        .decls()
        .iter()
        .filter(|decl| matches!(decl, Decl::Function(_)))
        .count();
    assert_eq!(records, 1, "Point lowers to one record");
    assert_eq!(functions, 1, "make_handler lowers to one function");

    let record = bindings
        .decls()
        .iter()
        .find_map(|decl| match decl {
            Decl::Record(record) => Some(record.as_ref()),
            _ => None,
        })
        .expect("record declaration");

    assert_eq!(record_method_counts(record), (1, 1));
}
