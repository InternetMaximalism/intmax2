package verifierCircuit

import (
	"github.com/consensys/gnark/frontend"
	"github.com/qope/gnark-plonky2-verifier/goldilocks"
	"github.com/qope/gnark-plonky2-verifier/types"
	"github.com/qope/gnark-plonky2-verifier/variables"
	"github.com/qope/gnark-plonky2-verifier/verifier"
)

type VerifierCircuit struct {
	PublicInputs            []goldilocks.Variable                     `gnark:",public"`
	Proof                   variables.Proof                   
	VerifierOnlyCircuitData variables.VerifierOnlyCircuitData `gnark:"-"`
	CommonCircuitData types.CommonCircuitData `gnark:"-"`
}

func (c *VerifierCircuit) Define(api frontend.API) error {
	verifierChip := verifier.NewVerifierChip(api, c.CommonCircuitData)
	verifierChip.Verify(c.Proof, c.PublicInputs, c.VerifierOnlyCircuitData)
	return nil
}
