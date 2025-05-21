# Finance reports analysis (Freereports)
This project is intended parse finance pdf reports and create `CSV` dataset.

## Installation
There are for now 2 installation method:
- using `pip` *(Recommended)*
- from source
### Using `pip`
Install in a python virtual environment launching
```bash
pip install freeports_analysis
```
### From source
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


## Quickstart
To start use the command provided with the library call
```bash
freeports -h
```
to see the options. All the option can be provided as environment variables.
If you want to use `freeports` as a python library write in you code
```python
import freeports_analysis as fra
```

## Usage
To have a complete overview of the package look at the [full documentation](https://gvoreste.github.io/analysis_finance_reports/).