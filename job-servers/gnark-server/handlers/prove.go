package handlers

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"log"
	"net/http"
	"time"

	verifierCircuit "gnark-server/circuit"
	"gnark-server/circuitData"
	"gnark-server/utils"

	"github.com/consensys/gnark-crypto/ecc"
	plonk_bn254 "github.com/consensys/gnark/backend/plonk/bn254"
	"github.com/consensys/gnark/frontend"
	"github.com/go-redis/redis/v8"
	"github.com/google/uuid"
	"github.com/qope/gnark-plonky2-verifier/types"
	"github.com/qope/gnark-plonky2-verifier/variables"
)

type ProveResult struct {
	PublicInputs []string `json:"publicInputs"`
	Proof        string   `json:"proof"`
}

type Status struct {
	Status string      `json:"status"`
	Result ProveResult `json:"result,omitempty"`
}

type State struct {
	CircuitData circuitData.CircuitData
	RedisClient *redis.Client
}

func (s *State) prove(jobId string, proofRaw types.ProofWithPublicInputsRaw) error {
	proofWithPis := variables.DeserializeProofWithPublicInputs(proofRaw)
	assignment := verifierCircuit.VerifierCircuit{
		Proof:                   proofWithPis.Proof,
		PublicInputs:            proofWithPis.PublicInputs,
		VerifierOnlyCircuitData: s.CircuitData.VerifierOnlyCircuitData,
	}
	witness, err := frontend.NewWitness(&assignment, ecc.BN254.ScalarField())
	ctx := context.Background()
	if err != nil {
		s.setStatus(ctx, jobId, Status{Status: "error"})
		return err
	}
	proof, err := plonk_bn254.Prove(&s.CircuitData.Ccs, &s.CircuitData.Pk, witness)
	if err != nil {
		s.setStatus(ctx, jobId, Status{Status: "error"})
		return err
	}
	proofHex := hex.EncodeToString(proof.MarshalSolidity())
	publicInputs, err := utils.ExtractPublicInputs(witness)
	if err != nil {
		s.setStatus(ctx, jobId, Status{Status: "error"})
		return err
	}
	publicInputsStr := make([]string, len(publicInputs))
	for i, bi := range publicInputs {
		publicInputsStr[i] = bi.String()
	}
	result := ProveResult{
		PublicInputs: publicInputsStr,
		Proof:        proofHex,
	}
	s.setStatus(ctx, jobId, Status{Status: "done", Result: result})
	log.Println("Prove done. jobId", jobId)
	return nil
}

func (s *State) setStatus(ctx context.Context, jobId string, status Status) error {
	statusJSON, err := json.Marshal(status)
	if err != nil {
		return err
	}
	return s.RedisClient.Set(ctx, jobId, statusJSON, 24*time.Hour).Err()
}

func (s *State) getStatus(ctx context.Context, jobId string) (Status, error) {
	var status Status
	statusJSON, err := s.RedisClient.Get(ctx, jobId).Result()
	if err != nil {
		return status, err
	}
	err = json.Unmarshal([]byte(statusJSON), &status)
	return status, err
}

func (s *State) StartProof(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	_jobId, err := uuid.NewRandom()
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	jobId := _jobId.String()
	var input types.ProofWithPublicInputsRaw
	if err := json.NewDecoder(r.Body).Decode(&input); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	s.setStatus(ctx, jobId, Status{Status: "in progress"})
	go s.prove(jobId, input)
	json.NewEncoder(w).Encode(map[string]string{"jobId": jobId})
	log.Println("StartProof", jobId)
}

func (s *State) GetProof(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	jobId := r.URL.Query().Get("jobId")
	log.Println("GetProof", jobId)
	_, err := uuid.Parse(jobId)
	if err != nil {
		http.Error(w, "Invalid JobId", http.StatusBadRequest)
		return
	}
	status, err := s.getStatus(ctx, jobId)
	if err == redis.Nil {
		http.Error(w, "job not found", http.StatusNotFound)
		return
	} else if err != nil {
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}
	json.NewEncoder(w).Encode(status)
}
