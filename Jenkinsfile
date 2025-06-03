pipeline {
    agent any

    stages {
        stage('Test') {
            steps {
                pytest
            }
        }
        stage('Lint') {
            steps {
                pylint
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