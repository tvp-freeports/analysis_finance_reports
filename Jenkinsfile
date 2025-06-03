pipeline {
    agent any

    stages {
        stage('Test') {
            steps {
                sh "pytest"
            }
        }
        stage('Lint') {
            steps {
                sh '''
                pylint . --exit-zero --output-format=parseable > pylint-report.txt
                pylint . --score=y --exit-zero --output-format=json > pylint-score.json
                '''
            }
        }
        stage('Build') {
            steps {
                sh "python -m build ."
            }
        }
        stage('Release') {
            steps {
                echo 'Deploying....'
            }
        }
    }
}