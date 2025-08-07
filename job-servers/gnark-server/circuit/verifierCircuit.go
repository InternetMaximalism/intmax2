package verifierCircuit

import (
	"fmt"
	"math/big"

	"github.com/consensys/gnark/frontend"
	"github.com/qope/gnark-plonky2-verifier/types"
	"github.com/qope/gnark-plonky2-verifier/variables"
	"github.com/qope/gnark-plonky2-verifier/verifier"
)

type VerifierCircuit struct {
	VerifierDigest frontend.Variable `gnark:"verifierDigest,public"`

	InputHash frontend.Variable `gnark:"inputHash,public"`

	VerifierData variables.VerifierOnlyCircuitData

	ProofWithPis variables.ProofWithPublicInputs

	CommonCircuitData types.CommonCircuitData `gnark:"-"`
}

func (c *VerifierCircuit) Define(api frontend.API) error {
	verifierChip := verifier.NewVerifierChip(api, c.CommonCircuitData)
	verifierChip.Verify(c.ProofWithPis.Proof, c.ProofWithPis.PublicInputs, c.VerifierData)

	publicInputs := c.ProofWithPis.PublicInputs

	if len(publicInputs) != 8 {
		return fmt.Errorf("expected 8 public inputs, got %d", len(publicInputs))
	}

	inputDigest := frontend.Variable(0)
	for i := 0; i < 8; i++ {
		limb := publicInputs[7-i].Limb
		inputDigest = api.Add(inputDigest, api.Mul(limb, frontend.Variable(new(big.Int).Lsh(big.NewInt(1), uint(32*i)))))
	}

	api.AssertIsEqual(c.InputHash, inputDigest)

	api.AssertIsEqual(c.VerifierDigest, c.VerifierData.CircuitDigest)

	return nil
}
