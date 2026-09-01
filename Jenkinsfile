// ============================================================================
//  PIPELINE NOT IN USE — kept because it will be turned back on shortly.
// ============================================================================
//
//  No Jenkins controller currently builds this repository. The file stays here, kept current,
//  rather than being deleted and rewritten from scratch at reactivation time: what deleting it
//  would lose is not the code — it is the thresholds, the trend graphs and the credentials that
//  have already been agreed on.
//
//  Being stopped, the pipeline is not verified against a real controller. Before turning it back
//  on: run it once on a scratch branch, and read the two notes marked ATTENTION below.
//
//  --- Historical note --------------------------------------------------------------------------
//  This file used to open with a warning claiming its hash was cited in
//  `docs/source/validation/general_methodology.rst`, and that modifying it would invalidate the
//  grants. The warning was verified and is false: no validation document, neither in this
//  repository nor in `analysis_finance_reports_formats/`, cites the `Jenkinsfile`. It was removed
//  because a wrong security warning is worse than no warning at all.
//
//  --- Relationship with the Makefile -----------------------------------------------------------
//  Every stage invokes a `make` target, never a command sequence of its own. That is deliberate:
//  the previous version of this file linted `src/` with pylint and ran `pytest tests/`, paths that
//  vanished with the Python engine, and nobody noticed because the pipeline described the build on
//  its own terms. A target, by contrast, is exercised daily by whoever is developing: if it
//  breaks, it breaks where somebody notices.
// ============================================================================

pipeline {
    agent any

    environment {
        PYPI_CREDENTIALS      = credentials('pypi-credentials')
        VENV_DIR              = 'venv/freeports-dev'
        COVERAGE_THRESHOLD_DOCS = '90.0'
        REPORTS_DIR           = 'reports'
        DOCS_DIR              = 'docs/build/html'
        TREND_DATA_DIR        = 'trend_data'
    }

    stages {
        stage('Checkout') {
            steps {
                script {
                    sh 'git fetch --tags'
                    def tag = sh(script: "git describe --tags --exact-match || echo ''", returnStdout: true).trim()
                    if (tag ==~ /^v?\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?$/) {
                        env.IS_RELEASE_TAG = 'true'
                        env.CURRENT_TAG = tag
                        echo "Detected release tag: ${env.CURRENT_TAG}"
                    } else {
                        env.IS_RELEASE_TAG = 'false'
                        echo "Not a valid release tag: ${tag}"
                    }
                }
            }
        }

        // `make init` creates the venv, wires up the git hooks and installs everything: the
        // engine (extension + binary), the tooling, the documentation dependencies.
        //
        // ATTENTION at reactivation: the agent needs a stable Rust toolchain (rustup) as well as
        // Python. The engine is a crate, no longer a pure Python package.
        stage('Setup') {
            steps {
                sh """
                    make init
                    mkdir -p ${REPORTS_DIR}
                """
            }
        }

        // clippy on the crate + ruff on the two Python packages. Pass/fail, with no score: the
        // out-of-ten score belonged to pylint, which this repository has not used since the
        // engine moved to Rust.
        stage('Lint') {
            steps {
                sh 'make lint 2>&1 | tee ${REPORTS_DIR}/lint.txt'
            }
            post {
                always {
                    archiveArtifacts "${REPORTS_DIR}/lint.txt"
                }
            }
        }

        // The crate's full suite: unit, integration, doctests.
        //
        // ATTENTION at reactivation: the coverage threshold the previous version enforced (70%)
        // is missing here. That is not an oversight — coverage was measured by `pytest-cov` over
        // Python code that no longer exists, and for Rust it would need a tool this repository
        // does not currently have. The candidate is `cargo llvm-cov`, to be added first as a
        // Makefile target (`coverage`) and only then as a threshold here: a threshold nobody can
        // reproduce locally is a threshold that gets switched off at the first failure.
        stage('Test') {
            steps {
                sh 'make check'
            }
        }

        stage('Build') {
            when {
                expression { return currentBuild.resultIsBetterOrEqualTo('SUCCESS') }
            }
            steps {
                // The crate's wheel (maturin) + wheels and sdists of the two tooling packages.
                sh 'make dist'
                archiveArtifacts 'dist/*'
            }
        }

        stage('Build Docs') {
            steps {
                script {
                    // The whole site, rustdoc included, plus the `sphinx.ext.coverage` report
                    // on the documented API.
                    sh '''
                        make docs
                        make docs-coverage
                    '''

                    docsCoverage = sh(
                        script: '''
                            . ${VENV_DIR}/bin/activate
                            python -c 'import re; \
                            text = open("docs/build/coverage/python.txt").read(); \
                            match = re.search(r"TOTAL\\s+\\|\\s+(\\d+\\.\\d+)%", text); \
                            print(match.group(1)) if match else print("0")' || echo "0"
                        ''',
                        returnStdout: true
                    ).trim().toFloat()

                    currentBuild.description = "Docs coverage: ${docsCoverage}%"

                    if (docsCoverage < Float.parseFloat(env.COVERAGE_THRESHOLD_DOCS)) {
                        error("Documentation coverage ${docsCoverage}% is below threshold of ${env.COVERAGE_THRESHOLD_DOCS}%")
                    }
                }
            }
            post {
                always {
                    publishHTML(
                        target: [
                            allowMissing: false,
                            alwaysLinkToLastBuild: true,
                            keepAll: true,
                            reportDir: "${DOCS_DIR}",
                            reportFiles: 'index.html',
                            reportName: 'Documentation'
                        ]
                    )
                    archiveArtifacts 'docs/build/coverage/python.txt'
                }
            }
        }

        stage('Release to PyPI') {
            when {
                allOf {
                    expression {
                        return currentBuild.result == null || currentBuild.resultIsBetterOrEqualTo('SUCCESS')
                    }
                    expression {
                        return env.IS_RELEASE_TAG == 'true'
                    }
                }
            }
            steps {
                script {
                    withCredentials([usernamePassword(credentialsId: 'pypi-credentials', usernameVariable: 'PYPI_USERNAME', passwordVariable: 'PYPI_PASSWORD')]) {
                        sh """
                            . ${VENV_DIR}/bin/activate
                            twine upload --username ${PYPI_USERNAME} --password ${PYPI_PASSWORD} dist/*
                        """
                    }
                }
            }
        }
    }

    post {
        always {
            script {
                sh "mkdir -p ${TREND_DATA_DIR}"

                def docsScore = currentBuild.description?.replaceAll(/.*Docs coverage: (\d+\.\d+)%.*/, '$1')
                def scoreFile = "${TREND_DATA_DIR}/docs_score.csv"
                writeFile file: scoreFile, text: "docs score\n${docsScore}\n", encoding: 'UTF-8'
                archiveArtifacts artifacts: scoreFile, onlyIfSuccessful: false
            }

            plot(
                csvFileName: 'plot-docscoverage.csv',
                title: 'Documentation Coverage Trend',
                yaxis: 'Coverage %',
                group: 'Quality of code',
                numBuilds: '50',
                description: 'Documentation coverage generated by `sphinx.ext.coverage`',
                csvSeries: [[file: "${TREND_DATA_DIR}/docs_score.csv"]],
                style: 'line',
                yaxisMinimum: '0',
                yaxisMaximum: '100'
            )

            // Workspace cleanup. The previous version ran `rm -rf ./*` followed by
            // `rm -rf ./.*`, which in a Jenkins working directory is a line not to leave lying
            // around: `./.*` includes `..`.
            cleanWs()
        }
    }
}
