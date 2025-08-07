package circuitData

import (
	"os"

	plonk_bn254 "github.com/consensys/gnark/backend/plonk/bn254"
	cs "github.com/consensys/gnark/constraint/bn254"
)

type CircuitData struct {
	Pk  plonk_bn254.ProvingKey
	Vk  plonk_bn254.VerifyingKey
	Ccs cs.SparseR1CS
}

func InitCircuitData() CircuitData {
	var data CircuitData
	{
		fVk, err := os.Open("data/verifying.key")
		if err != nil {
			panic(err)
		}
		_, _ = data.Vk.ReadFrom(fVk)
		defer fVk.Close()
	}
	{
		fPk, err := os.Open("data/proving.key")
		if err != nil {
			panic(err)
		}
		_, _ = data.Pk.ReadFrom(fPk)
		defer fPk.Close()
	}
	{
		fCs, err := os.Open("data/circuit.r1cs")
		if err != nil {
			panic(err)
		}
		_, _ = data.Ccs.ReadFrom(fCs)
		defer fCs.Close()
	}
	return data
}
