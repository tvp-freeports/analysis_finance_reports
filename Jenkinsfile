pipeline {
    agent any

    stages {
        stage('Test') {
            steps {
                echo 'pytest'
            }
        }
        stage('Lint') {
            steps {
                echo 'pylint'
            }
        }
        stage('Build') {
            steps {
                echo 'python -m build .'
            }
        }
        stage('Release') {
            steps {
                echo 'Deploying....'
            }
        }
    }
}