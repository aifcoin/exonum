use std::marker::PhantomData;

use crypto::Hash;

use super::{BaseIndex, Snapshot, Fork, StorageValue};

use self::key::{ProofMapKey, DBKey, ChildKind};
use self::node::{Node, BranchNode};
use self::proof::{RootProofNode, ProofNode, BranchProofNode};

#[cfg(test)]
mod tests;
mod key;
mod node;
mod proof;

pub struct ProofMapIndex<T, K, V> {
    base: BaseIndex<T>,
    _k: PhantomData<K>,
    _v: PhantomData<V>,
}

enum RemoveResult {
    KeyNotFound,
    Leaf,
    Branch((DBKey, Hash)),
    UpdateHash(Hash),
}

impl<T, K, V> ProofMapIndex<T, K, V> {
    pub fn new(prefix: Vec<u8>, base: T) -> Self {
        ProofMapIndex {
            base: BaseIndex::new(prefix, base),
            _k: PhantomData,
            _v: PhantomData
        }
    }
}

impl<T, K, V> ProofMapIndex<T, K, V> where T: AsRef<Snapshot>,
                                           K: ProofMapKey,
                                           V: StorageValue {
    fn root_prefix(&self) -> Option<Vec<u8>> {
        unimplemented!();
    }

    pub fn root_hash(&self) -> Hash {
        unimplemented!();
        // match self.root_node()? {
        //     Some((root_db_key, Node::Leaf(value))) => {
        //         Ok(hash(&[root_db_key.as_slice(), value.hash().as_ref()].concat()))
        //     }
        //     Some((_, Node::Branch(branch))) => Ok(branch.hash()),
        //     None => Ok(Hash::zero()),
        // }
    }

    fn root_node(&self) -> Option<(DBKey, Node<V>)> {
        unimplemented!();
        // let out = match self.root_prefix()? {
        //     Some(db_key) => {
        //         let node = self.get_node_unchecked(&db_key)?;
        //         Some((db_key, node))
        //     }
        //     None => None,
        // };
        // Ok(out)
    }

    fn get_node_unchecked(&self, key: &DBKey) -> Node<V> {
        // TODO: unwraps?
        match key.is_leaf() {
            true => Node::Leaf(self.base.get(key).unwrap()),
            false => Node::Branch(self.base.get(key).unwrap())
        }
    }

    fn construct_proof(&self,
                       current_branch: &BranchNode,
                       searched_slice: &DBKey) -> Option<ProofNode<V>> {

        let child_slice = current_branch.child_slice(searched_slice.get(0));
        // FIXME: child_slice.from = searched_slice.from;
        let c_pr_l = child_slice.common_prefix(searched_slice);
        debug_assert!(c_pr_l > 0);
        if c_pr_l < child_slice.len() {
            return None
        }

        let res: ProofNode<V> = match self.get_node_unchecked(&child_slice) {
            Node::Leaf(child_value) => ProofNode::Leaf(child_value),
            Node::Branch(child_branch) => {
                let l_s = child_branch.child_slice(ChildKind::Left);
                let r_s = child_branch.child_slice(ChildKind::Right);
                let suf_searched_slice = searched_slice.suffix(c_pr_l);
                let proof_from_level_below: Option<ProofNode<V>> =
                    self.construct_proof(&child_branch, &suf_searched_slice);

                if let Some(child_proof) = proof_from_level_below {
                    let child_proof_pos = suf_searched_slice.get(0);
                    let neighbour_child_hash = *child_branch.child_hash(!child_proof_pos);
                    match child_proof_pos {
                        ChildKind::Left => {
                            ProofNode::Branch(BranchProofNode::LeftBranch {
                                left_hash: Box::new(child_proof),
                                right_hash: neighbour_child_hash,
                                left_key: l_s.suffix(c_pr_l),
                                right_key: r_s.suffix(c_pr_l),
                            })
                        }
                        ChildKind::Right => {
                            ProofNode::Branch(BranchProofNode::RightBranch {
                                left_hash: neighbour_child_hash,
                                right_hash: Box::new(child_proof),
                                left_key: l_s.suffix(c_pr_l),
                                right_key: r_s.suffix(c_pr_l),
                            })
                        }
                    }
                } else {
                    let l_h = *child_branch.child_hash(ChildKind::Left); //copy
                    let r_h = *child_branch.child_hash(ChildKind::Right);//copy
                    ProofNode::Branch(BranchProofNode::BranchKeyNotFound {
                        left_hash: l_h,
                        right_hash: r_h,
                        left_key: l_s.suffix(c_pr_l),
                        right_key: r_s.suffix(c_pr_l),
                    })
                    // proof of exclusion of a key, because none of child slices is a prefix(searched_slice)
                }
            }
        };
        Some(res)
    }

    pub fn get_proof(&self, key: &K) -> RootProofNode<V> {
        let searched_slice = DBKey::leaf(key);

        let res: RootProofNode<V> = match self.root_node() {
            Some((root_db_key, Node::Leaf(root_value))) => {
                if searched_slice == root_db_key {
                    RootProofNode::LeafRootInclusive(root_db_key,
                                                     root_value)
                } else {
                    RootProofNode::LeafRootExclusive(root_db_key,
                                                     root_value.hash())
                }
            }
            Some((root_db_key, Node::Branch(branch))) => {
                let root_slice = root_db_key;
                let l_s = branch.child_slice(ChildKind::Left);
                let r_s = branch.child_slice(ChildKind::Right);

                let c_pr_l = root_slice.common_prefix(&searched_slice);
                if c_pr_l == root_slice.len() {
                    let suf_searched_slice = searched_slice.suffix(c_pr_l);
                    let proof_from_level_below: Option<ProofNode<V>> =
                        self.construct_proof(&branch, &suf_searched_slice);

                    if let Some(child_proof) = proof_from_level_below {
                        let child_proof_pos = suf_searched_slice.get(0);
                        let neighbour_child_hash = *branch.child_hash(!child_proof_pos);
                        match child_proof_pos {
                            ChildKind::Left => {
                                RootProofNode::Branch(BranchProofNode::LeftBranch {
                                    left_hash: Box::new(child_proof),
                                    right_hash: neighbour_child_hash,
                                    left_key: l_s,
                                    right_key: r_s,
                                })
                            }
                            ChildKind::Right => {
                                RootProofNode::Branch(BranchProofNode::RightBranch {
                                    left_hash: neighbour_child_hash,
                                    right_hash: Box::new(child_proof),
                                    left_key: l_s,
                                    right_key: r_s,
                                })
                            }
                        }
                    } else {
                        let l_h = *branch.child_hash(ChildKind::Left); //copy
                        let r_h = *branch.child_hash(ChildKind::Right);//copy
                        RootProofNode::Branch(BranchProofNode::BranchKeyNotFound {
                            left_hash: l_h,
                            right_hash: r_h,
                            left_key: l_s,
                            right_key: r_s,
                        })
                        // proof of exclusion of a key, because none of child slices is a prefix(searched_slice)
                    }
                } else {
                    // if common prefix length with root_slice is less than root_slice length
                    let l_h = *branch.child_hash(ChildKind::Left); //copy
                    let r_h = *branch.child_hash(ChildKind::Right);//copy
                    RootProofNode::Branch(BranchProofNode::BranchKeyNotFound {
                        left_hash: l_h,
                        right_hash: r_h,
                        left_key: l_s,
                        right_key: r_s,
                    })
                    // proof of exclusion of a key, because root_slice != prefix(searched_slice)
                }
            }
            None => return RootProofNode::Empty,
        };
        res
    }

    pub fn get(&self, key: &K) -> Option<V> {
        self.base.get(&DBKey::leaf(key))
    }

    pub fn contains(&self, key: &K) -> bool {
        self.base.contains(&DBKey::leaf(key))
    }

}

impl<'a, K, V> ProofMapIndex<&'a mut Fork, K, V> where K: ProofMapKey,
                                                       V: StorageValue {
    fn insert_leaf(&mut self, key: &DBKey, value: V) -> Hash {
        debug_assert!(key.is_leaf());
        let hash = value.hash();
        self.base.put(key, value);
        hash
    }

    // Inserts a new node as child of current branch and returns updated hash
    // or if a new node has more short key returns a new key length
    fn insert_branch(&mut self,
                     parent: &BranchNode,
                     key_slice: &DBKey,
                     value: V) -> (Option<u16>, Hash) {
        let child_slice = parent.child_slice(key_slice.get(0));
        // FIXME: child_slice.from = key_slice.from;
        // If the slice is fully fit in key then there is a two cases
        let i = child_slice.common_prefix(key_slice);
        if child_slice.len() == i {
            // check that child is leaf to avoid unnecessary read
            if child_slice.is_leaf() {
                // there is a leaf in branch and we needs to update its value
                let hash = self.insert_leaf(key_slice, value);
                (None, hash)
            } else {
                match self.get_node_unchecked(&child_slice) {
                    Node::Leaf(_) => {
                        unreachable!("Something went wrong!");
                    }
                    // There is a child in branch and we needs to lookup it recursively
                    Node::Branch(mut branch) => {
                        let (j, h) = self.insert_branch(&branch, &key_slice.suffix(i), value);
                        match j {
                            Some(j) => {
                                branch.set_child(key_slice.get(i), &key_slice.suffix(i).truncate(j), &h)
                            }
                            None => branch.set_child_hash(key_slice.get(i), &h),
                        };
                        let hash = branch.hash();
                        self.base.put(&child_slice, branch);
                        (None, hash)
                    }
                }
            }
        } else {
            // A simple case of inserting a new branch
            let suffix_slice = key_slice.suffix(i);
            let mut new_branch = BranchNode::empty();
            // Add a new leaf
            let hash = self.insert_leaf(&suffix_slice, value);
            new_branch.set_child(suffix_slice.get(0), &suffix_slice, &hash);
            // Move current branch
            new_branch.set_child(child_slice.get(i),
                                 &child_slice.suffix(i),
                                 parent.child_hash(key_slice.get(0)));

            let hash = new_branch.hash();
            self.base.put(&key_slice.truncate(i), new_branch);
            (Some(i), hash)
        }
    }

    pub fn put(&mut self, key: &K, value: V) {
        let key_slice = DBKey::leaf(key);
        match self.root_node() {
            Some((prefix, Node::Leaf(prefix_data))) => {
                let prefix_slice = prefix;
                let i = prefix_slice.common_prefix(&key_slice);

                let leaf_hash = self.insert_leaf(&key_slice, value);
                if i < key_slice.len() {
                    let mut branch = BranchNode::empty();
                    branch.set_child(key_slice.get(i), &key_slice.suffix(i), &leaf_hash);
                    branch.set_child(prefix_slice.get(i),
                                     &prefix_slice.suffix(i),
                                     &prefix_data.hash());
                    let new_prefix = key_slice.truncate(i);
                    self.base.put(&new_prefix, branch);
                }
            }
            Some((prefix, Node::Branch(mut branch))) => {
                let prefix_slice = prefix;
                let i = prefix_slice.common_prefix(&key_slice);

                if i == prefix_slice.len() {
                    let suffix_slice = key_slice.suffix(i);
                    // Just cut the prefix and recursively descent on.
                    let (j, h) = self.insert_branch(&branch, &suffix_slice, value);
                    match j {
                        Some(j) => {
                            branch.set_child(suffix_slice.get(0), &suffix_slice.truncate(j), &h)
                        }
                        None => branch.set_child_hash(suffix_slice.get(0), &h),
                    };
                    self.base.put(&prefix_slice, branch);
                } else {
                    // Inserts a new branch and adds current branch as its child
                    let hash = self.insert_leaf(&key_slice, value);
                    let mut new_branch = BranchNode::empty();
                    new_branch.set_child(prefix_slice.get(i), &prefix_slice.suffix(i), &branch.hash());
                    new_branch.set_child(key_slice.get(i), &key_slice.suffix(i), &hash);
                    // Saves a new branch
                    let new_prefix = prefix_slice.truncate(i);
                    self.base.put(&new_prefix, new_branch);
                }
            }
            None => { self.insert_leaf(&key_slice, value); }
        }
    }

    fn remove_node(&mut self, parent: &BranchNode, key_slice: &DBKey) -> RemoveResult {
        let child_slice = parent.child_slice(key_slice.get(0));
        // FIXME: child_slice.from = key_slice.from;
        let i = child_slice.common_prefix(key_slice);

        if i == child_slice.len() {
            match self.get_node_unchecked(&child_slice) {
                Node::Leaf(_) => {
                    self.base.delete(key_slice);
                    return RemoveResult::Leaf
                }
                Node::Branch(mut branch) => {
                    let suffix_slice = key_slice.suffix(i);
                    match self.remove_node(&branch, &suffix_slice) {
                        RemoveResult::Leaf => {
                            let child = !suffix_slice.get(0);
                            let key = branch.child_slice(child);
                            let hash = branch.child_hash(child);

                            self.base.delete(&child_slice);

                            return RemoveResult::Branch((key, *hash))
                        }
                        RemoveResult::Branch((key, hash)) => {
                            // FIXME: let mut new_child_slice = DBKey::from_db_key(key.as_ref());
                            // FIXME: new_child_slice.from = suffix_slice.from;

                            branch.set_child(suffix_slice.get(0), &key, &hash);
                            let h = branch.hash();
                            self.base.put(&child_slice, branch);
                            return RemoveResult::UpdateHash(h)
                        }
                        RemoveResult::UpdateHash(hash) => {
                            branch.set_child_hash(suffix_slice.get(0), &hash);
                            let h = branch.hash();
                            self.base.put(&child_slice, branch);
                            return RemoveResult::UpdateHash(h)
                        }
                        RemoveResult::KeyNotFound => {
                            return RemoveResult::KeyNotFound
                        }
                    }
                }
            }
        }
        RemoveResult::KeyNotFound
    }

    pub fn delete(&mut self, key: &K) {
        let key_slice = DBKey::leaf(key);
        match self.root_node() {
            // If we have only on leaf, then we just need to remove it (if any)
            Some((prefix, Node::Leaf(_))) => {
                let key = key_slice;
                if key == prefix {
                    self.base.delete(&key);
                }
            },
            Some((prefix, Node::Branch(mut branch))) => {
                // Truncate prefix
                let i = prefix.common_prefix(&key_slice);
                if i == prefix.len() {
                    let suffix_slice = key_slice.suffix(i);
                    match self.remove_node(&branch, &suffix_slice) {
                        RemoveResult::Leaf => self.base.delete(&prefix),
                        RemoveResult::Branch((key, hash)) => {
                            // FIXME let mut new_child_slice = DBKey::from_db_key(key.as_ref());
                            // FIXME new_child_slice.from = suffix_slice.from;
                            branch.set_child(suffix_slice.get(0), &key, &hash);
                            self.base.put(&prefix, branch);
                        }
                        RemoveResult::UpdateHash(hash) => {
                            branch.set_child_hash(suffix_slice.get(0), &hash);
                            self.base.put(&prefix, branch);
                        }
                        RemoveResult::KeyNotFound => return
                    }
                }
            }
            None => (),
        }
    }

    pub fn clear(&mut self) {
        self.base.clear()
    }
}
