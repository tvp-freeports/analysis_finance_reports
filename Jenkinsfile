pipeline {
    agent any

    environment {        
        // PyPI credentials should be stored in Jenkins credentials store
        PYPI_CREDENTIALS = credentials('pypi-credentials')
        VENV_DIR = "venv/freeports-dev"
        LINT_SCORE_THRESHOLD = '3.0'
        COVERAGE_THRESHOLD = '1.0'
        COVERAGE_THRESHOLD_DOCS = '1.0'
        REPORTS_DIR = 'reports'
        DOCS_DIR = 'docs/build/html'
        
    }
    stages {
        stage('Checkout') {
            steps {
                // Verify if this is a tagged build
                script {
                    isTagged = env.TAG_NAME != null
                    if (isTagged) {
                        echo "Building tagged release: ${env.TAG_NAME}"
                    }
                }
            }
        }
        stage('Setup') {
            steps {
                sh """
                    contrib/init.sh
                    mkdir -p ${REPORTS_DIR}
                """
            }
        }
        stage('Lint') {
             steps {
                script {
                    // Run pylint and capture the score
                    lintOutput = sh(
                        script: """
                            . ${VENV_DIR}/bin/activate
                            pylint --exit-zero --output-format=json src > ${REPORTS_DIR}/pylint.json || true
                            pylint --exit-zero src | tee ${REPORTS_DIR}/pylint.txt
                        """,
                        returnStdout: true
                    )
                    
                    // Extract the score from the output
                    lintScore = sh(
                        script: """
                            . ${VENV_DIR}/bin/activate
                            python -c \"import re; print(re.search(r'Your code has been rated at (\\d+\\.\\d+)/10', open('${REPORTS_DIR}/pylint.txt').read()).group(1))\"
                        """,
                        returnStdout: true
                    ).trim()
                    
                    // Store the score for the trend graph
                    currentBuild.description = "Lint score: ${lintScore}/10"
                    
                    // Fail if score is below threshold
                    if (Float.parseFloat(lintScore) < Float.parseFloat(env.LINT_SCORE_THRESHOLD)) {
                        error("Lint score ${lintScore} is below threshold of ${env.LINT_SCORE_THRESHOLD}")
                    }
                }
            }
            post {
                always {
                    // Archive lint reports
                    archiveArtifacts "${REPORTS_DIR}/pylint.*"
                    // Record the lint score for trend analysis
                    recordIssues(
                        tools: [pylint(pattern: '${REPORTS_DIR}/pylint.txt')],
                        healthy: 8, unhealthy: 6, minimumSeverity: 'LOW'
                    )
                }
            }
        }
        
        stage('Test') {
            steps {
                sh """
                    . ${VENV_DIR}/bin/activate
                    pytest tests/ --cov=src --cov-report=xml:${REPORTS_DIR}/coverage.xml --junitxml=${REPORTS_DIR}/test-results.xml  # Adjust test directory
                """
            }
            post {
                always {
                    script {
                        // These steps will run even if tests fail
                        coverageOutput = sh(
                            script: """
                                . ${VENV_DIR}/bin/activate
                                python -c 'import xml.etree.ElementTree as ET; print(ET.parse(\"${REPORTS_DIR}/coverage.xml\").getroot().attrib[\"line-rate\"])' || echo \"0\"
                            """,
                            returnStdout: true
                        ).trim()
                        coveragePercent = (Float.parseFloat(coverageOutput) * 100).round(2)
                        currentBuild.description = "${currentBuild.description} | Test Coverage: ${coveragePercent}%"
                        
                        // Fail if coverage is below threshold (only check if tests passed)
                        if (currentBuild.resultIsBetterOrEqualTo('SUCCESS')) {
                            if (coveragePercent < Float.parseFloat(env.COVERAGE_THRESHOLD)) {
                                error("Test coverage ${coveragePercent}% is below threshold of ${env.COVERAGE_THRESHOLD}%")
                            }
                        }
                    }
                    
                    junit "${REPORTS_DIR}/test-results.xml"
                    cobertura "${REPORTS_DIR}/coverage.xml"
                }
            }
        }
        stage('Build Docs') {
            steps {
                script {
                    sh """
                        . ${VENV_DIR}/bin/activate
                        cd docs && make html
                    """
                    
                    // Check documentation coverage (requires sphinx-coverage)
                    docsCoverage = sh(
                        script: """
                            . ${VENV_DIR}/bin/activate
                            python -c \"import re; \
                                text = open('docs/_build/coverage/python.txt').read(); \
                                match = re.search(r'Total\\s+(\\d+\\.\\d+)%', text); \
                                print(match.group(1)) if match else print('0')\" || echo \"0\"
                        """,
                        returnStdout: true
                    ).trim().toFloat()
                    
                    currentBuild.description = "${currentBuild.description} | Docs: ${docsCoverage}%"
                    
                    if (docsCoverage < Float.parseFloat(env.DOCS_COVERAGE_THRESHOLD)) {
                        error("Documentation coverage ${docsCoverage}% is below threshold of ${env.DOCS_COVERAGE_THRESHOLD}%")
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
                            reportDir: 'docs/build/html',
                            reportFiles: 'index.html',
                            reportName: 'API Documentation'
                        ]
                    )
                    archiveArtifacts 'docs/build/coverage/python.txt'
                }
            }
        }
        
        stage('Build') {
            when {
                expression { return currentBuild.resultIsBetterOrEqualTo('SUCCESS') }
            }
            steps {
                sh """
                    . ${VENV_DIR}/bin/activate
                    python -m build
                """
                archiveArtifacts 'dist/*'
            }
        }
        
        stage('Release to PyPI') {
            when {
                allOf {
                    expression { return isTagged }
                    expression { return currentBuild.resultIsBetterOrEqualTo('SUCCESS') }
                }
            }
            steps {
                script {
                    // Verify the tag follows semantic versioning (optional but recommended)
                    if (!(env.TAG_NAME ==~ /^v?\d+\.\d+\.\d+(-.+)?$/)) {
                        error("Tag ${env.TAG_NAME} doesn't follow semantic versioning pattern")
                    }
                    
                    // Upload to PyPI
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
            // Clean up virtual environment
            sh 'rm -rf ./*'
            sh 'rm -rf ./.*'

            // Generate lint trend graph (requires Plot plugin)
            script {
                // Store metrics for trend graphs
                def metrics = [
                    'lint': currentBuild.description?.replaceAll(/.*Lint: (\d+\.\d+).*/, '$1'),
                    'test': currentBuild.description?.replaceAll(/.*Test Coverage: (\d+\.\d+)%.*/, '$1'),
                    'docs': currentBuild.description?.replaceAll(/.*Docs: (\d+\.\d+)%.*/, '$1')
                ]
                metrics.each { name, value ->
                    if (value?.isNumber()) {
                        writeFile file: "${name}_score.dat", text: "${env.BUILD_NUMBER}\t${value}\n"
                        archiveArtifacts "${name}_score.dat"
                    }
                }
            }
            // Individual trend graphs
            plot(
                title: 'Pylint Score Trend',
                yaxis: 'Score (0-10)',
                csvSeries: [[file: 'lint_score.dat', label: 'Lint Score', style: 'line', color: 'blue']],
                yaxisMinimum: 0,
                yaxisMaximum: 10
            )
            
            plot(
                title: 'Test Coverage Trend',
                yaxis: 'Coverage %',
                csvSeries: [[file: 'test_score.dat', label: 'Test Coverage', style: 'line', color: 'green']],
                yaxisMinimum: 0,
                yaxisMaximum: 100
            )
            
            plot(
                title: 'Documentation Coverage Trend',
                yaxis: 'Coverage %',
                csvSeries: [[file: 'docs_score.dat', label: 'Docs Coverage', style: 'line', color: 'purple']],
                yaxisMinimum: 0,
                yaxisMaximum: 100
            )
        
           
        }
    }
}
