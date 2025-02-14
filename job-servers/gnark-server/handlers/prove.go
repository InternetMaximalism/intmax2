package handlers

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
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

const (
	redisKeyPrefix = "gnark_proof_result:"
	expiration     = 24 * time.Hour
)

type ProveResult struct {
	PublicInputs []string `json:"publicInputs"`
	Proof        string   `json:"proof"`
}

type ProofResponse struct {
	Success      bool         `json:"success"`
	Proof        *ProveResult `json:"proof"`
	ErrorMessage *string      `json:"errorMessage"`
}

type State struct {
	CircuitData circuitData.CircuitData
	RedisClient *redis.Client
}

func getRedisKey(jobId string) string {
	return fmt.Sprintf("%s%s", redisKeyPrefix, jobId)
}

func (s *State) setProofResponse(ctx context.Context, jobId string, response ProofResponse) error {
	responseJSON, err := json.Marshal(response)
	if err != nil {
		return err
	}
	return s.RedisClient.Set(ctx, getRedisKey(jobId), responseJSON, expiration).Err()
}

func (s *State) getProofResponse(ctx context.Context, jobId string) (ProofResponse, error) {
	var response ProofResponse
	responseJSON, err := s.RedisClient.Get(ctx, getRedisKey(jobId)).Result()
	if err != nil {
		return response, err
	}
	err = json.Unmarshal([]byte(responseJSON), &response)
	return response, err
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
		errMsg := err.Error()
		resp := ProofResponse{
			Success:      false,
			Proof:        nil,
			ErrorMessage: &errMsg,
		}
		s.setProofResponse(ctx, jobId, resp)
		return err
	}
	proof, err := plonk_bn254.Prove(&s.CircuitData.Ccs, &s.CircuitData.Pk, witness)
	if err != nil {
		errMsg := err.Error()
		resp := ProofResponse{
			Success:      false,
			Proof:        nil,
			ErrorMessage: &errMsg,
		}
		s.setProofResponse(ctx, jobId, resp)
		return err
	}
	proofHex := hex.EncodeToString(proof.MarshalSolidity())
	publicInputs, err := utils.ExtractPublicInputs(witness)
	if err != nil {
		errMsg := err.Error()
		resp := ProofResponse{
			Success:      false,
			Proof:        nil,
			ErrorMessage: &errMsg,
		}
		s.setProofResponse(ctx, jobId, resp)
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
	resp := ProofResponse{
		Success: true,
		Proof:   &result,
	}
	s.setProofResponse(ctx, jobId, resp)
	log.Println("Prove done. jobId", jobId)
	return nil
}

func (s *State) StartProof(w http.ResponseWriter, r *http.Request) {
	_jobId, err := uuid.NewRandom()
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	jobId := _jobId.String()

	var rawInput struct {
		Proof string `json:"proof"`
	}
	if err := json.NewDecoder(r.Body).Decode(&rawInput); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	var input types.ProofWithPublicInputsRaw
	if err := json.Unmarshal([]byte(rawInput.Proof), &input); err != nil {
		http.Error(w, "Failed to parse proof JSON: "+err.Error(), http.StatusBadRequest)
		return
	}

	resp := ProofResponse{
		Success: true,
		Proof:   nil,
	}
	if err := s.setProofResponse(context.Background(), jobId, resp); err != nil {
		log.Printf("Failed to store proof response in Redis: %v\n", err)
	}

	go s.prove(jobId, input)
	json.NewEncoder(w).Encode(map[string]string{"jobId": jobId})
	log.Println("StartProof", jobId)
}

func (s *State) GetProof(w http.ResponseWriter, r *http.Request) {
	jobId := r.URL.Query().Get("jobId")
	log.Println("GetProof", jobId)
	_, err := uuid.Parse(jobId)
	if err != nil {
		http.Error(w, "Invalid JobId", http.StatusBadRequest)
		return
	}
	response, err := s.getProofResponse(r.Context(), jobId)
	if err == redis.Nil {
		http.Error(w, "job not found", http.StatusNotFound)
		return
	} else if err != nil {
		http.Error(w, "Internal server error", http.StatusInternalServerError)
		return
	}
	json.NewEncoder(w).Encode(response)
}
