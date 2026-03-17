from Bio.PDB import PDBParser
import math

# Create parser
parser = PDBParser(QUIET=True)

# Parse the PDB file
structure = parser.get_structure('5WB7', '/Users/sujiangwen/sandbox/LLM/speechless.ai/Autonomous-Agents/octo-sandbox/crates/octo-eval/datasets/gaia_files/7dd30055-0198-452e-8c25-f73dbe27dcb8.pdb')

# Get all atoms in order as they appear in the file
atoms = []
for model in structure:
    for chain in model:
        for residue in chain:
            for atom in residue:
                atoms.append(atom)

# Get first two atoms
atom1 = atoms[0]
atom2 = atoms[1]

print(f'First atom: {atom1.get_name()} of residue {atom1.get_parent().get_resname()} {atom1.get_parent().get_id()[1]} chain {atom1.get_parent().get_parent().get_id()}')
print(f'  Coordinates: {atom1.get_coord()}')
print(f'Second atom: {atom2.get_name()} of residue {atom2.get_parent().get_resname()} {atom2.get_parent().get_id()[1]} chain {atom2.get_parent().get_parent().get_id()}')
print(f'  Coordinates: {atom2.get_coord()}')

# Calculate distance
coord1 = atom1.get_coord()
coord2 = atom2.get_coord()
distance = math.sqrt(sum((c1 - c2) ** 2 for c1, c2 in zip(coord1, coord2)))

print(f'Distance: {distance} Angstroms')
print(f'Distance in picometers: {distance * 100}')
print(f'Rounded to nearest picometer: {round(distance * 100)} pm')
print(f'Answer in Angstroms (rounded to nearest picometer): {round(distance * 100) / 100} Angstroms')
