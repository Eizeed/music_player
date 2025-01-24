use std::path::PathBuf;

pub fn path_buf_vec_to_string(paths: &[PathBuf]) -> String {
    let mut res = String::new();
    for path in paths {
        res.push_str(&format!("'{}', ", path.to_str().unwrap()));
    }

    res.pop();
    res.pop();

    return res;
}
