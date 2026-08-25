package main

import "crypto/rand"

// Separado para o main não depender de crypto/rand diretamente e o teste poder
// substituir a fonte se algum dia precisar.
func leituraAleatoria(p []byte) (int, error) { return rand.Read(p) }
