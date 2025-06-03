pipeline {
    agent any

    environment {        
        // PyPI credentials should be stored in Jenkins credentials store
        PYPI_CREDENTIALS = credentials('pypi-credentials')
        VENV_DIR = "venv/freeports-dev"
        LINT_SCORE_THRESHOLD = '3.0'
        LINT_REPORT_DIR = 'reports'
        
    }
    stages {
        stage('Checkout') {
            steps {
                checkout scm
                
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
                    contribute/init.sh
                    mkdir -p ${LINT_REPORT_DIR}
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
                            pylint --exit-zero --output-format=json ./ > ${LINT_REPORT_DIR}/pylint.json || true
                            pylint --exit-zero ./ | tee ${LINT_REPORT_DIR}/pylint.txt
                        """,
                        returnStdout: true
                    )
                    
                    // Extract the score from the output
                    lintScore = sh(
                        script: """
                            . ${VENV_DIR}/bin/activate
                            python -c \"import re; print(re.search(r'Your code has been rated at (\\d+\\.\\d+)/10', open('${LINT_REPORT_DIR}/pylint.txt').read()).group(1))\"
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
                    archiveArtifacts "${LINT_REPORT_DIR}/pylint.*"
                    
                    // Record the lint score for trend analysis
                    recordIssues(
                        tools: [pylint(pattern: '${LINT_REPORT_DIR}/pylint.txt')],
                        healthy: 8, unhealthy: 6, minimumSeverity: 'LOW'
                    )
                }
            }
        }
        
        stage('Test') {
            steps {
                sh """
                    . ${VENV_DIR}/bin/activate
                    pytest tests/ --cov=src --cov-report=xml --junitxml=${LINT_REPORT_DIR}/test-results.xml  # Adjust test directory
                """
            }
            post {
                always {
                    junit "${LINT_REPORT_DIR}/test-results.xml"
                    cobertura '**/coverage.xml'
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
                archiveArtifacts 'dist/*'
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
            plot(
                title: 'Pylint Score Trend',
                yaxis: 'Score',
                series: [
                    [file: 'lint_score.dat', label: 'Lint Score', style: 'line']
                ],
                group: 'code-quality',
                useDescriptions: true
            )
            
            // Append current lint score to trend file
            script {
                def lintScore = currentBuild.description?.replaceAll(/.*Lint score: (\d+\.\d+).*/, '$1')
                if (lintScore && lintScore.isNumber()) {
                    writeFile file: 'lint_score.dat', text: "${env.BUILD_NUMBER}\t${lintScore}\n"
                    archiveArtifacts 'lint_score.dat'
                }
            }
        }
        
    }
}
