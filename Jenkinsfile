pipeline {
    agent any

    environment {        
        // PyPI credentials should be stored in Jenkins credentials store
        PYPI_CREDENTIALS = credentials('pypi-credentials')
        VENV_DIR = "venv/freeports-dev"
        LINT_SCORE_THRESHOLD = '3.0'
        COVERAGE_TRASHOLD = '1.0'
        REPORTS_DIR = 'reports'
        
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
                    //archiveArtifacts "${REPORTS_DIR}/pylint.*"
                    echo "Done"
                    // // Record the lint score for trend analysis
                    // recordIssues(
                    //     tools: [pylint(pattern: '${REPORTS_DIR}/pylint.txt')],
                    //     healthy: 8, unhealthy: 6, minimumSeverity: 'LOW'
                    // )
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
                                python -c \"import xml.etree.ElementTree as ET; print(ET.parse('${REPORTS_DIR}/coverage.xml').getroot().attrib['line-rate'])\" || echo "0"
                            """,
                            returnStdout: true
                        ).trim()
                        coveragePercent = (Float.parseFloat(coverageOutput) * 100).round(2)
                        currentBuild.description = "${currentBuild.description} | Coverage: ${coveragePercent}%"
                        
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
        
        stage('Build') {
            steps {
                sh """
                    . ${VENV_DIR}/bin/activate
                    python -m build
                """
                
                // Archive the built distribution files
                //archiveArtifacts 'dist/*'
            }
        }
        
        stage('Release to PyPI') {
            when {
                expression { return isTagged }
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
                    
                    echo "Successfully released version ${env.TAG_NAME} to PyPI"
                }
            }
        }
    }
    post {
        always {
            // Clean up virtual environment
            sh 'rm -rf ${VENV_DIR}'

            // Generate lint trend graph (requires Plot plugin)
            script {
                // Store lint score data
                def lintScore = currentBuild.description?.replaceAll(/.*Lint: (\d+\.\d+).*/, '$1')
                if (lintScore && lintScore.isNumber()) {
                    writeFile file: 'lint_score.dat', text: "${env.BUILD_NUMBER}\t${lintScore}\n"
                    archiveArtifacts 'lint_score.dat'
                }
                
                // Store coverage data
                def coveragePercent = currentBuild.description?.replaceAll(/.*Coverage: (\d+\.\d+)%.*/, '$1')
                if (coveragePercent && coveragePercent.isNumber()) {
                    writeFile file: 'coverage_score.dat', text: "${env.BUILD_NUMBER}\t${coveragePercent}\n"
                    archiveArtifacts 'coverage_score.dat'
                }
            }
        
            // Separate graph for Pylint scores
            plot(
                title: 'Pylint Score Trend',
                yaxis: 'Score (out of 10)',
                series: [
                    [file: 'lint_score.dat', label: 'Lint Score', style: 'line', color: 'blue']
                ],
                group: 'code-quality',
                useDescriptions: true,
                yaxisMinimum: 0,
                yaxisMaximum: 10
            )
            
            // Separate graph for Test Coverage
            plot(
                title: 'Test Coverage Trend',
                yaxis: 'Coverage (%)',
                series: [
                    [file: 'coverage_score.dat', label: 'Coverage', style: 'line', color: 'green']
                ],
                group: 'code-quality',
                useDescriptions: true,
                yaxisMinimum: 0,
                yaxisMaximum: 100
            )
            
            // Optional: Combined graph for quick overview
            plot(
                title: 'Code Quality Overview',
                yaxis: 'Score',
                series: [
                    [file: 'lint_score.dat', label: 'Lint Score (scaled)', style: 'line', color: 'blue', 
                    transformation: { it * 10 }], // Scale 0-10 to 0-100 for comparison
                    [file: 'coverage_score.dat', label: 'Test Coverage', style: 'line', color: 'green']
                ],
                group: 'code-quality',
                useDescriptions: true,
                yaxisMinimum: 0,
                yaxisMaximum: 100
            )
        }
    }
}
