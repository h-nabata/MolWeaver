use std::collections::HashMap;
use std::fmt;

use glam::Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AtomId(u64);

impl AtomId {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BondId(u64);

impl BondId {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct Atom {
    pub id: AtomId,
    pub element: String,
    pub position: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct Bond {
    pub id: BondId,
    pub a: AtomId,
    pub b: AtomId,
}

#[derive(Debug, Clone)]
pub struct Molecule {
    pub name: String,
    atoms: HashMap<AtomId, Atom>,
    atom_order: Vec<AtomId>,
    bonds: HashMap<BondId, Bond>,
    valence_counts: HashMap<AtomId, usize>,
    next_atom_id: u64,
    next_bond_id: u64,
}

impl Molecule {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            atoms: HashMap::new(),
            atom_order: Vec::new(),
            bonds: HashMap::new(),
            valence_counts: HashMap::new(),
            next_atom_id: 1,
            next_bond_id: 1,
        }
    }

    pub fn atom_count(&self) -> usize {
        self.atoms.len()
    }

    pub fn atoms_in_order(&self) -> impl Iterator<Item = &Atom> {
        self.atom_order.iter().filter_map(|id| self.atoms.get(id))
    }

    pub fn atom_ids(&self) -> Vec<AtomId> {
        self.atom_order.clone()
    }

    pub fn get_atom(&self, id: AtomId) -> Option<&Atom> {
        self.atoms.get(&id)
    }

    pub fn insert_atom(&mut self, element: String, position: [f32; 3]) -> AtomId {
        let id = AtomId(self.next_atom_id);
        self.next_atom_id += 1;
        let atom = Atom {
            id,
            element,
            position,
        };
        self.atoms.insert(id, atom);
        self.atom_order.push(id);
        self.valence_counts.insert(id, 0);
        id
    }

    pub fn insert_atom_with_id(
        &mut self,
        id: AtomId,
        element: String,
        position: [f32; 3],
        order_index: Option<usize>,
    ) -> AtomId {
        self.next_atom_id = self.next_atom_id.max(id.0 + 1);
        let atom = Atom {
            id,
            element,
            position,
        };
        self.atoms.insert(id, atom);
        if let Some(index) = order_index {
            let clamped = index.min(self.atom_order.len());
            self.atom_order.insert(clamped, id);
        } else {
            self.atom_order.push(id);
        }
        self.valence_counts.entry(id).or_insert(0);
        id
    }

    pub fn remove_atom(&mut self, id: AtomId) -> Option<RemovedAtom> {
        let atom = self.atoms.remove(&id)?;
        let order_index = self
            .atom_order
            .iter()
            .position(|entry| *entry == id)
            .unwrap_or(self.atom_order.len());
        if order_index < self.atom_order.len() {
            self.atom_order.remove(order_index);
        }
        let bonds: Vec<Bond> = self
            .bonds
            .values()
            .filter(|bond| bond.a == id || bond.b == id)
            .cloned()
            .collect();
        for bond in &bonds {
            self.bonds.remove(&bond.id);
            self.decrement_valence(bond.a);
            self.decrement_valence(bond.b);
        }
        self.valence_counts.remove(&id);
        Some(RemovedAtom {
            atom,
            order_index,
            bonds,
        })
    }

    pub fn set_atom_position(&mut self, id: AtomId, position: [f32; 3]) -> Option<()> {
        let atom = self.atoms.get_mut(&id)?;
        atom.position = position;
        Some(())
    }

    pub fn add_bond(&mut self, a: AtomId, b: AtomId) -> Result<BondId, String> {
        self.ensure_atoms_exist(a, b)?;
        if self.bond_between(a, b).is_some() {
            return Err("bond already exists".to_string());
        }
        self.ensure_valence_available(a)?;
        self.ensure_valence_available(b)?;
        let id = BondId(self.next_bond_id);
        self.next_bond_id += 1;
        let bond = Bond { id, a, b };
        self.bonds.insert(id, bond);
        self.increment_valence(a);
        self.increment_valence(b);
        Ok(id)
    }

    pub fn insert_bond_with_id(
        &mut self,
        id: BondId,
        a: AtomId,
        b: AtomId,
    ) -> Result<BondId, String> {
        self.ensure_atoms_exist(a, b)?;
        self.next_bond_id = self.next_bond_id.max(id.0 + 1);
        if self.bond_between(a, b).is_some() {
            return Err("bond already exists".to_string());
        }
        self.ensure_valence_available(a)?;
        self.ensure_valence_available(b)?;
        let bond = Bond { id, a, b };
        self.bonds.insert(id, bond);
        self.increment_valence(a);
        self.increment_valence(b);
        Ok(id)
    }

    pub fn remove_bond(&mut self, id: BondId) -> Option<Bond> {
        let bond = self.bonds.remove(&id)?;
        self.decrement_valence(bond.a);
        self.decrement_valence(bond.b);
        Some(bond)
    }

    pub fn bond_between(&self, a: AtomId, b: AtomId) -> Option<BondId> {
        self.bonds
            .values()
            .find(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))
            .map(|bond| bond.id)
    }

    pub fn bonds(&self) -> impl Iterator<Item = &Bond> {
        self.bonds.values()
    }

    fn ensure_atoms_exist(&self, a: AtomId, b: AtomId) -> Result<(), String> {
        if !self.atoms.contains_key(&a) || !self.atoms.contains_key(&b) {
            return Err("atom does not exist".to_string());
        }
        Ok(())
    }

    fn ensure_valence_available(&self, atom_id: AtomId) -> Result<(), String> {
        let atom = self
            .atoms
            .get(&atom_id)
            .ok_or_else(|| "atom does not exist".to_string())?;
        let max_valence = max_valence(&atom.element);
        let current = self.valence_counts.get(&atom_id).copied().unwrap_or(0);
        if current + 1 > max_valence {
            return Err(format!(
                "valence exceeded for {} (max {})",
                atom.element, max_valence
            ));
        }
        Ok(())
    }

    fn increment_valence(&mut self, atom_id: AtomId) {
        let entry = self.valence_counts.entry(atom_id).or_insert(0);
        *entry += 1;
    }

    fn decrement_valence(&mut self, atom_id: AtomId) {
        if let Some(entry) = self.valence_counts.get_mut(&atom_id) {
            *entry = entry.saturating_sub(1);
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemovedAtom {
    pub atom: Atom,
    pub order_index: usize,
    pub bonds: Vec<Bond>,
}

#[derive(Debug, Clone)]
pub enum Command {
    InsertAtom {
        element: String,
        position: [f32; 3],
        atom_id: Option<AtomId>,
        order_index: Option<usize>,
    },
    DeleteAtom {
        atom_id: AtomId,
        removed: Option<RemovedAtom>,
    },
    AddBond {
        atom_a: AtomId,
        atom_b: AtomId,
        bond_id: Option<BondId>,
    },
    RemoveBond {
        bond_id: BondId,
        removed: Option<Bond>,
    },
    MoveAtom {
        atom_id: AtomId,
        from: [f32; 3],
        to: [f32; 3],
    },
}

impl Command {
    pub fn apply(&mut self, molecule: &mut Molecule) -> Result<(), String> {
        match self {
            Command::InsertAtom {
                element,
                position,
                atom_id,
                order_index,
            } => {
                let index = order_index.get_or_insert(molecule.atom_order.len());
                let id = if let Some(id) = atom_id {
                    molecule.insert_atom_with_id(*id, element.clone(), *position, Some(*index))
                } else {
                    let new_id = molecule.insert_atom(element.clone(), *position);
                    *atom_id = Some(new_id);
                    new_id
                };
                *atom_id = Some(id);
                Ok(())
            }
            Command::DeleteAtom { atom_id, removed } => {
                let result = molecule
                    .remove_atom(*atom_id)
                    .ok_or_else(|| "atom not found".to_string())?;
                *removed = Some(result);
                Ok(())
            }
            Command::AddBond {
                atom_a,
                atom_b,
                bond_id,
            } => {
                if molecule.bond_between(*atom_a, *atom_b).is_some() {
                    return Err("bond already exists".to_string());
                }
                let id = if let Some(id) = bond_id {
                    molecule.insert_bond_with_id(*id, *atom_a, *atom_b)?
                } else {
                    let new_id = molecule.add_bond(*atom_a, *atom_b)?;
                    *bond_id = Some(new_id);
                    new_id
                };
                *bond_id = Some(id);
                Ok(())
            }
            Command::RemoveBond { bond_id, removed } => {
                let bond = molecule
                    .remove_bond(*bond_id)
                    .ok_or_else(|| "bond not found".to_string())?;
                *removed = Some(bond);
                Ok(())
            }
            Command::MoveAtom { atom_id, to, .. } => {
                molecule
                    .set_atom_position(*atom_id, *to)
                    .ok_or_else(|| "atom not found".to_string())?;
                Ok(())
            }
        }
    }

    pub fn undo(&mut self, molecule: &mut Molecule) -> Result<(), String> {
        match self {
            Command::InsertAtom {
                atom_id: Some(atom_id),
                ..
            } => {
                molecule
                    .remove_atom(*atom_id)
                    .ok_or_else(|| "atom not found".to_string())?;
                Ok(())
            }
            Command::DeleteAtom { removed, .. } => {
                let removed = removed
                    .clone()
                    .ok_or_else(|| "missing undo data".to_string())?;
                molecule.insert_atom_with_id(
                    removed.atom.id,
                    removed.atom.element,
                    removed.atom.position,
                    Some(removed.order_index),
                );
                for bond in removed.bonds {
                    molecule.insert_bond_with_id(bond.id, bond.a, bond.b)?;
                }
                Ok(())
            }
            Command::AddBond {
                bond_id: Some(bond_id),
                ..
            } => {
                molecule
                    .remove_bond(*bond_id)
                    .ok_or_else(|| "bond not found".to_string())?;
                Ok(())
            }
            Command::RemoveBond { removed, .. } => {
                let bond = removed
                    .clone()
                    .ok_or_else(|| "missing undo data".to_string())?;
                molecule.insert_bond_with_id(bond.id, bond.a, bond.b)?;
                Ok(())
            }
            Command::MoveAtom { atom_id, from, .. } => {
                molecule
                    .set_atom_position(*atom_id, *from)
                    .ok_or_else(|| "atom not found".to_string())?;
                Ok(())
            }
            _ => Err("command missing data".to_string()),
        }
    }

    pub fn merge_with(&mut self, other: &Command) -> bool {
        match (self, other) {
            (
                Command::MoveAtom {
                    atom_id: a_id,
                    to: a_to,
                    ..
                },
                Command::MoveAtom { atom_id, to, .. },
            ) if a_id == atom_id => {
                *a_to = *to;
                true
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandHistory {
    undo: Vec<Command>,
    redo: Vec<Command>,
    capacity: usize,
}

impl CommandHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn execute(
        &mut self,
        mut command: Command,
        molecule: &mut Molecule,
    ) -> Result<Command, String> {
        command.apply(molecule)?;
        self.redo.clear();
        if let Some(last) = self.undo.last_mut() {
            if last.merge_with(&command) {
                return Ok(last.clone());
            }
        }
        self.undo.push(command.clone());
        if self.undo.len() > self.capacity {
            self.undo.remove(0);
        }
        Ok(command)
    }

    pub fn undo(&mut self, molecule: &mut Molecule) -> Result<Option<Command>, String> {
        if let Some(mut command) = self.undo.pop() {
            command.undo(molecule)?;
            self.redo.push(command.clone());
            return Ok(Some(command));
        }
        Ok(None)
    }

    pub fn redo(&mut self, molecule: &mut Molecule) -> Result<Option<Command>, String> {
        if let Some(mut command) = self.redo.pop() {
            command.apply(molecule)?;
            self.undo.push(command.clone());
            return Ok(Some(command));
        }
        Ok(None)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct XyzError {
    details: String,
}

impl XyzError {
    fn new(details: impl Into<String>) -> Self {
        Self {
            details: details.into(),
        }
    }
}

impl fmt::Display for XyzError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl std::error::Error for XyzError {}

pub fn parse_xyz(contents: &str) -> Result<Molecule, XyzError> {
    let mut lines = contents.lines();
    let count_line = lines
        .next()
        .ok_or_else(|| XyzError::new("missing atom count"))?;
    let atom_count: usize = count_line
        .trim()
        .parse()
        .map_err(|_| XyzError::new("invalid atom count"))?;

    let comment_line = lines
        .next()
        .ok_or_else(|| XyzError::new("missing comment line"))?;
    let name = comment_line.trim().to_string();

    let mut molecule = Molecule::new(name);
    for (index, line) in lines.enumerate() {
        if molecule.atoms.len() >= atom_count {
            break;
        }
        let mut parts = line.split_whitespace();
        let element = parts
            .next()
            .ok_or_else(|| XyzError::new(format!("missing element at line {}", index + 3)))?
            .to_string();
        let x: f32 = parts
            .next()
            .ok_or_else(|| XyzError::new(format!("missing x at line {}", index + 3)))?
            .parse()
            .map_err(|_| XyzError::new(format!("invalid x at line {}", index + 3)))?;
        let y: f32 = parts
            .next()
            .ok_or_else(|| XyzError::new(format!("missing y at line {}", index + 3)))?
            .parse()
            .map_err(|_| XyzError::new(format!("invalid y at line {}", index + 3)))?;
        let z: f32 = parts
            .next()
            .ok_or_else(|| XyzError::new(format!("missing z at line {}", index + 3)))?
            .parse()
            .map_err(|_| XyzError::new(format!("invalid z at line {}", index + 3)))?;
        molecule.insert_atom(element, [x, y, z]);
    }

    if molecule.atoms.len() != atom_count {
        return Err(XyzError::new("atom count does not match data lines"));
    }

    Ok(molecule)
}

pub fn element_color(element: &str) -> [f32; 3] {
    match element.trim().to_ascii_uppercase().as_str() {
        "H" => [1.0, 1.0, 1.0],
        "C" => [0.2, 0.2, 0.2],
        "N" => [0.2, 0.2, 1.0],
        "O" => [1.0, 0.2, 0.2],
        _ => [0.7, 0.7, 0.7],
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BondInstance {
    pub midpoint: [f32; 3],
    pub direction: [f32; 3],
    pub length: f32,
}

pub fn bond_instance_from_positions(a: [f32; 3], b: [f32; 3]) -> BondInstance {
    let a_vec = Vec3::from_array(a);
    let b_vec = Vec3::from_array(b);
    let delta = b_vec - a_vec;
    let length = delta.length();
    let direction = if length > 0.0 {
        (delta / length).to_array()
    } else {
        [0.0, 1.0, 0.0]
    };
    BondInstance {
        midpoint: ((a_vec + b_vec) * 0.5).to_array(),
        direction,
        length,
    }
}

fn max_valence(element: &str) -> usize {
    match element.trim().to_ascii_uppercase().as_str() {
        "H" => 1,
        "C" => 4,
        "N" => 3,
        "O" => 2,
        "F" | "CL" | "BR" | "I" => 1,
        "P" => 5,
        "S" => 6,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_xyz_valid() {
        let data = "2\nwater\nO 0.0 0.0 0.0\nH 0.0 1.0 0.0\n";
        let molecule = parse_xyz(data).expect("parse xyz");
        assert_eq!(molecule.atom_count(), 2);
        assert_eq!(molecule.name, "water");
        let ids = molecule.atom_ids();
        assert_eq!(molecule.get_atom(ids[0]).unwrap().element, "O");
    }

    #[test]
    fn parse_xyz_invalid_count() {
        let data = "3\ncomment\nH 0 0 0\n";
        let err = parse_xyz(data).unwrap_err();
        assert!(err.to_string().contains("atom count"));
    }

    #[test]
    fn parse_xyz_invalid_number() {
        let data = "1\ncomment\nH a b c\n";
        let err = parse_xyz(data).unwrap_err();
        assert!(err.to_string().contains("invalid x"));
    }

    #[test]
    fn element_color_mapping() {
        assert_eq!(element_color("H"), [1.0, 1.0, 1.0]);
        assert_eq!(element_color("C"), [0.2, 0.2, 0.2]);
        assert_eq!(element_color("N"), [0.2, 0.2, 1.0]);
        assert_eq!(element_color("O"), [1.0, 0.2, 0.2]);
        assert_eq!(element_color("Xe"), [0.7, 0.7, 0.7]);
    }

    #[test]
    fn command_insert_undo() {
        let mut molecule = Molecule::new("test");
        let mut history = CommandHistory::new(10);
        let command = Command::InsertAtom {
            element: "H".into(),
            position: [0.0, 0.0, 0.0],
            atom_id: None,
            order_index: None,
        };
        let executed = history.execute(command, &mut molecule).unwrap();
        assert_eq!(molecule.atom_count(), 1);
        let id = match executed {
            Command::InsertAtom { atom_id, .. } => atom_id.unwrap(),
            _ => panic!("expected insert"),
        };
        history.undo(&mut molecule).unwrap();
        assert!(molecule.get_atom(id).is_none());
        history.redo(&mut molecule).unwrap();
        assert!(molecule.get_atom(id).is_some());
    }

    #[test]
    fn command_delete_with_bonds() {
        let mut molecule = Molecule::new("test");
        let a = molecule.insert_atom("C".into(), [0.0, 0.0, 0.0]);
        let b = molecule.insert_atom("H".into(), [1.0, 0.0, 0.0]);
        let bond_id = molecule.add_bond(a, b).unwrap();
        let mut history = CommandHistory::new(10);
        let command = Command::DeleteAtom {
            atom_id: a,
            removed: None,
        };
        history.execute(command, &mut molecule).unwrap();
        assert!(molecule.get_atom(a).is_none());
        assert!(molecule.remove_bond(bond_id).is_none());
        history.undo(&mut molecule).unwrap();
        assert!(molecule.get_atom(a).is_some());
        assert!(molecule.bond_between(a, b).is_some());
    }

    #[test]
    fn command_bond_add_remove() {
        let mut molecule = Molecule::new("test");
        let a = molecule.insert_atom("C".into(), [0.0, 0.0, 0.0]);
        let b = molecule.insert_atom("H".into(), [1.0, 0.0, 0.0]);
        let mut history = CommandHistory::new(10);
        let command = Command::AddBond {
            atom_a: a,
            atom_b: b,
            bond_id: None,
        };
        let executed = history.execute(command, &mut molecule).unwrap();
        let bond_id = match executed {
            Command::AddBond { bond_id, .. } => bond_id.unwrap(),
            _ => panic!("expected bond"),
        };
        assert!(molecule.bond_between(a, b).is_some());
        history.undo(&mut molecule).unwrap();
        assert!(molecule.bond_between(a, b).is_none());
        history.redo(&mut molecule).unwrap();
        assert!(molecule.bond_between(a, b).is_some());
        let remove = Command::RemoveBond {
            bond_id,
            removed: None,
        };
        history.execute(remove, &mut molecule).unwrap();
        assert!(molecule.bond_between(a, b).is_none());
    }

    #[test]
    fn command_bond_valence_rejected() {
        let mut molecule = Molecule::new("test");
        let c = molecule.insert_atom("C".into(), [0.0, 0.0, 0.0]);
        let h1 = molecule.insert_atom("H".into(), [1.0, 0.0, 0.0]);
        let h2 = molecule.insert_atom("H".into(), [0.0, 1.0, 0.0]);
        let h3 = molecule.insert_atom("H".into(), [0.0, 0.0, 1.0]);
        let h4 = molecule.insert_atom("H".into(), [-1.0, 0.0, 0.0]);
        let h5 = molecule.insert_atom("H".into(), [0.0, -1.0, 0.0]);
        let mut history = CommandHistory::new(10);
        history
            .execute(
                Command::AddBond {
                    atom_a: c,
                    atom_b: h1,
                    bond_id: None,
                },
                &mut molecule,
            )
            .unwrap();
        history
            .execute(
                Command::AddBond {
                    atom_a: c,
                    atom_b: h2,
                    bond_id: None,
                },
                &mut molecule,
            )
            .unwrap();
        history
            .execute(
                Command::AddBond {
                    atom_a: c,
                    atom_b: h3,
                    bond_id: None,
                },
                &mut molecule,
            )
            .unwrap();
        history
            .execute(
                Command::AddBond {
                    atom_a: c,
                    atom_b: h4,
                    bond_id: None,
                },
                &mut molecule,
            )
            .unwrap();
        let result = history.execute(
            Command::AddBond {
                atom_a: c,
                atom_b: h5,
                bond_id: None,
            },
            &mut molecule,
        );
        assert!(result.is_err());
        assert!(molecule.bond_between(c, h5).is_none());
        assert!(!history.can_redo());
    }

    #[test]
    fn failed_command_does_not_mutate() {
        let mut molecule = Molecule::new("test");
        let a = molecule.insert_atom("H".into(), [0.0, 0.0, 0.0]);
        let b = molecule.insert_atom("H".into(), [1.0, 0.0, 0.0]);
        let mut history = CommandHistory::new(10);
        history
            .execute(
                Command::AddBond {
                    atom_a: a,
                    atom_b: b,
                    bond_id: None,
                },
                &mut molecule,
            )
            .unwrap();
        let before_bond = molecule.bond_between(a, b);
        let result = history.execute(
            Command::AddBond {
                atom_a: a,
                atom_b: b,
                bond_id: None,
            },
            &mut molecule,
        );
        assert!(result.is_err());
        assert_eq!(molecule.bond_between(a, b), before_bond);
    }

    #[test]
    fn command_move_atom() {
        let mut molecule = Molecule::new("test");
        let a = molecule.insert_atom("C".into(), [0.0, 0.0, 0.0]);
        let mut history = CommandHistory::new(10);
        let command = Command::MoveAtom {
            atom_id: a,
            from: [0.0, 0.0, 0.0],
            to: [1.0, 2.0, 3.0],
        };
        history.execute(command, &mut molecule).unwrap();
        assert_eq!(molecule.get_atom(a).unwrap().position, [1.0, 2.0, 3.0]);
        history.undo(&mut molecule).unwrap();
        assert_eq!(molecule.get_atom(a).unwrap().position, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn undo_redo_stack_behavior() {
        let mut molecule = Molecule::new("test");
        let mut history = CommandHistory::new(10);
        let command = Command::InsertAtom {
            element: "H".into(),
            position: [0.0, 0.0, 0.0],
            atom_id: None,
            order_index: None,
        };
        history.execute(command, &mut molecule).unwrap();
        assert!(history.can_undo());
        history.undo(&mut molecule).unwrap();
        assert!(history.can_redo());
        let command = Command::InsertAtom {
            element: "O".into(),
            position: [1.0, 0.0, 0.0],
            atom_id: None,
            order_index: None,
        };
        history.execute(command, &mut molecule).unwrap();
        assert!(!history.can_redo());
    }

    #[test]
    fn bond_instance_direction_and_length() {
        let instance = bond_instance_from_positions([0.0, 0.0, 0.0], [0.0, 2.0, 0.0]);
        assert_eq!(instance.length, 2.0);
        assert_eq!(instance.direction, [0.0, 1.0, 0.0]);
        assert_eq!(instance.midpoint, [0.0, 1.0, 0.0]);
    }
}
