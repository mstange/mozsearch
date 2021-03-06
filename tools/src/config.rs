use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use std::collections::BTreeMap;
use std::collections::HashMap;

use rustc_serialize::json::{self, Json};
use rustc_serialize::Decodable;

use git2::{Oid, Repository};

#[derive(RustcDecodable, RustcEncodable)]
pub struct TreeConfigPaths {
    pub index_path: String,
    pub files_path: String,
    pub git_path: Option<String>,
    pub git_blame_path: Option<String>,
    pub objdir_path: String,
    pub hg_root: Option<String>,
}

pub struct GitData {
    pub repo: Repository,
    pub blame_repo: Option<Repository>,

    pub blame_map: HashMap<Oid, Oid>, // Maps repo OID to blame_repo OID.
    pub hg_map: HashMap<Oid, String>, // Maps repo OID to Hg rev.
}

pub struct TreeConfig {
    pub paths: TreeConfigPaths,
    pub git: Option<GitData>,
}

pub struct Config {
    pub trees: BTreeMap<String, TreeConfig>,
    pub mozsearch_path: String,
}

pub fn get_git(tree_config: &TreeConfig) -> Result<&GitData, &'static str> {
    match &tree_config.git {
        &Some(ref git) => Ok(git),
        &None => Err("History data unavailable"),
    }
}

pub fn get_git_path(tree_config: &TreeConfig) -> Result<&str, &'static str> {
    match &tree_config.paths.git_path {
        &Some(ref git_path) => Ok(git_path),
        &None => Err("History data unavailable"),
    }
}

pub fn get_hg_root(tree_config: &TreeConfig) -> String {
    // For temporary backwards compatibility, produce the m-c root if
    // there isn't one specified. We can remove this once all relevant
    // deployed config.json files have an explicit hg root, and make
    // this return an Option<&str> instead.
    match &tree_config.paths.hg_root {
        &Some(ref hg_root) => hg_root.clone(),
        &None => String::from("https://hg.mozilla.org/mozilla-central"),
    }
}

fn index_blame(_repo: &Repository, blame_repo: &Repository) -> (HashMap<Oid, Oid>, HashMap<Oid, String>) {
    let mut walk = blame_repo.revwalk().unwrap();
    walk.push_head().unwrap();

    let mut blame_map = HashMap::new();
    let mut hg_map = HashMap::new();
    for r in walk {
        let oid = r.unwrap();
        let commit = blame_repo.find_commit(oid).unwrap();

        let msg = commit.message().unwrap();
        let pieces = msg.split_whitespace().collect::<Vec<_>>();

        let orig_oid = Oid::from_str(pieces[1]).unwrap();
        blame_map.insert(orig_oid, commit.id());

        if pieces.len() > 2 {
            let hg_id = pieces[3].to_owned();
            hg_map.insert(orig_oid, hg_id);
        }
    }

    (blame_map, hg_map)
}

pub fn load(config_path: &str, need_indexes: bool) -> Config {
    let config_file = File::open(config_path).unwrap();
    let mut reader = BufReader::new(&config_file);
    let mut input = String::new();
    reader.read_to_string(&mut input).unwrap();
    let config = Json::from_str(&input).unwrap();

    let mut obj = config.as_object().unwrap().clone();

    let mozsearch_json = obj.remove("mozsearch_path").unwrap();
    let mozsearch = mozsearch_json.as_string().unwrap();

    let trees_obj = obj.get("trees").unwrap().as_object().unwrap().clone();
    
    let mut trees = BTreeMap::new();
    for (tree_name, tree_config) in trees_obj {
        let mut decoder = json::Decoder::new(tree_config);
        let paths = TreeConfigPaths::decode(&mut decoder).unwrap();

        let git = match (&paths.git_path, &paths.git_blame_path) {
            (&Some(ref git_path), &Some(ref git_blame_path)) => {
                let repo = Repository::open(&git_path).unwrap();
                let blame_repo = Repository::open(&git_blame_path).unwrap();

                let (blame_map, hg_map) = if need_indexes {
                    index_blame(&repo, &blame_repo)
                } else {
                    (HashMap::new(), HashMap::new())
                };

                Some(GitData {
                    repo: repo,
                    blame_repo: Some(blame_repo),
                    blame_map: blame_map,
                    hg_map: hg_map,
                })
            },
            (&Some(ref git_path), &None) => {
                Some(GitData {
                    repo: Repository::open(&git_path).unwrap(),
                    blame_repo: None,
                    blame_map: HashMap::new(),
                    hg_map: HashMap::new(),
                })
            },
            _ => None,
        };

        trees.insert(tree_name, TreeConfig {
            paths: paths,
            git: git,
        });
    }

    Config { trees: trees, mozsearch_path: mozsearch.to_owned() }
}
