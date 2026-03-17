import pandas as pd

# Read the Excel file
file_path = "/Users/sujiangwen/sandbox/LLM/speechless.ai/Autonomous-Agents/octo-sandbox/crates/octo-eval/datasets/gaia_files/32102e3e-d12a-4209-9163-7b3a104efe5d.xlsx"

# Read all sheets
xl = pd.ExcelFile(file_path)
print("Sheet names:", xl.sheet_names)

# Read the first sheet to explore
df = pd.read_excel(file_path, sheet_name=0)
print("\nFirst few rows:")
print(df.head())
print("\nColumns:", df.columns.tolist())
print("\nDataset shape:", df.shape)
