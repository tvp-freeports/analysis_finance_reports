============
Installation
============

There are for now 2 installation method:
- using `pip` *(Recommended)*
- from source
-----------
Using `pip`
-----------
Install in a python virtual environment launching
```bash
pip install freeports_analysis
```
-----------
From source
-----------
> Requirements:
> You need to have the python `build` package. 
> You can install it in your virtual environment with `pip install build`
1. Clone the repository:
```bash
git clone https://github.com/GVoreste/analysis_finance_reports.git
```
2. `cd` into the created directory
```bash
cd analysis_finance_reports
```
3. build the package
```bash
python -m build .
```
4. install local package
```bash
pip install .
```
5. enjoy