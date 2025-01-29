package main

import (
	"flag"
	"fmt"
	"net/http"
	"os"

	"context"
	"gnark-server/config"
	gnarkContext "gnark-server/context"
	"gnark-server/handlers"
	"time"

	"github.com/redis/go-redis/v9"
)

func main() {
	circuitName := flag.String("circuit", "", "circuit name")
	flag.Parse()

	if *circuitName == "" {
		fmt.Println("Please provide circuit name")
		os.Exit(1)
	}
	fmt.Printf("Preparing server with circuit: %s\n", *circuitName)

	cfg, err := config.LoadConfig()
	if err != nil {
		fmt.Printf("Failed to load config: %v\n", err)
		return
	}

	redisClient := redis.NewClient(&redis.Options{
		Addr:     cfg.GetRedisAddr(),
		Password: "password",
		DB:       0,
	})

	ctxRedis, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	_, err = redisClient.Ping(ctxRedis).Result()
	if err != nil {
		fmt.Printf("Failed to load config: %v\n", err)
		return
	}

	circuitData := gnarkContext.InitCircuitData(*circuitName)
	ctx := &handlers.CircuitData{
		CircuitData: circuitData,
		RedisClient: redisClient,
	}

	http.HandleFunc("/health", handlers.HealthHandler)
	http.HandleFunc("/start-proof", ctx.StartProof)
	http.HandleFunc("/get-proof", ctx.GetProof)
	fmt.Printf("Server is running on port %s...", cfg.ServerPort)
	addr := fmt.Sprintf(":%s", cfg.ServerPort)
	if err := http.ListenAndServe(addr, nil); err != nil {
		panic(err)
	}
}
