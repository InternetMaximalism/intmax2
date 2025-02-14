package circuitData

import (
	"os"

	plonk_bn254 "github.com/consensys/gnark/backend/plonk/bn254"
	cs "github.com/consensys/gnark/constraint/bn254"
	"github.com/qope/gnark-plonky2-verifier/types"
	"github.com/qope/gnark-plonky2-verifier/variables"
)

type CircuitData struct {
   Pk plonk_bn254.ProvingKey
   Vk plonk_bn254.VerifyingKey
   Ccs cs.SparseR1CS
   VerifierOnlyCircuitData variables.VerifierOnlyCircuitData
}

func InitCircuitData(circuitName string) CircuitData{
	var data CircuitData
	{
		fVk, err := os.Open("data/"+circuitName+"/verifying.key")
		if err != nil {
			panic(err)
		}
		_, _ = data.Vk.ReadFrom(fVk)
		defer fVk.Close()
	}
	{
		fPk, err := os.Open("data/"+circuitName+"/proving.key")
		if err != nil {
			panic(err)
		}
		_, _ = data.Pk.ReadFrom(fPk)
		defer fPk.Close()
	}
	{
		fCs, err := os.Open("data/"+circuitName+"/circuit.r1cs")
		if err != nil {
			panic(err)
		}
		_, _ = data.Ccs.ReadFrom(fCs)
		defer fCs.Close()
	}
	{
		data.VerifierOnlyCircuitData = variables.DeserializeVerifierOnlyCircuitData(types.ReadVerifierOnlyCircuitData("data/"+circuitName+"/verifier_only_circuit_data.json"))
	}
	return data
}
