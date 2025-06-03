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
                sh "pylint ."
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