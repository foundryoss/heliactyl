package billing

import (
	"context"
	"math/rand"
	"time"

	"github.com/sirupsen/logrus"

	"NightLightd/internal/container"
	"NightLightd/internal/monitoring"
)

// RUTracker samples container stats periodically and accumulates Resource Units (RU).
// Rules implemented:
// - 1 RU = 1 GB RAM * 1 hour
// - Sampling interval: randomized between 5 and 10 seconds
// - Store only last sample + cumulative totals in container state (no time series)
// - On RU limit reached, freeze container with message "RU_FREEZE: LIMIT REACHED"

const (
	minSampleSec      = 5
	maxSampleSec      = 10
	defaultCPUToGB    = 0.25 // 1 full CPU averaged counts as 0.25 GB-equivalent
	defaultDiskWeight = 0.01 // each GB of storage contributes 0.01 GB-equivalent toward RU
)

type RUTracker struct {
	log              *logrus.Logger
	monitor          *monitoring.Monitor
	containerManager *container.Manager
	tracked          map[string]string // containerID -> volumeID
	mu               chan struct{}     // lightweight mutex via channel
	stop             chan struct{}
}

func NewRUTracker(log *logrus.Logger, monitor *monitoring.Monitor, cm *container.Manager) *RUTracker {
	r := &RUTracker{
		log:              log,
		monitor:          monitor,
		containerManager: cm,
		tracked:          make(map[string]string),
		mu:               make(chan struct{}, 1),
		stop:             make(chan struct{}),
	}
	// seed rand
	rand.Seed(time.Now().UnixNano())
	return r
}

// Run starts the sampling loop. This blocks until ctx is cancelled or Stop() is called.
func (r *RUTracker) Run(ctx context.Context) {
	r.log.Info("RUTracker started")
	for {
		// Randomized sleep between minSampleSec and maxSampleSec
		interval := time.Duration(minSampleSec+rand.Intn(maxSampleSec-minSampleSec+1)) * time.Second
		select {
		case <-time.After(interval):
			// ensure tracked set is up-to-date from container states
			r.syncTrackedWithStates()
			// sample each tracked container
			r.sampleAll(interval)
		case <-ctx.Done():
			r.log.Info("RUTracker stopping due to context cancellation")
			return
		case <-r.stop:
			r.log.Info("RUTracker stopped")
			return
		}
	}
}

// Stop signals the tracker to stop.
func (r *RUTracker) Stop() {
	select {
	case <-r.stop:
		// already closed
	default:
		close(r.stop)
	}
}

// syncTrackedWithStates inspects container manager states and ensures tracked set matches
func (r *RUTracker) syncTrackedWithStates() {
	r.lock()
	defer r.unlock()

	states := r.containerManager.GetAllStates()
	// add new ones
	for vid, st := range states {
		if st.ContainerID != "" && !st.Frozen && st.RULimit > 0 {
			if _, ok := r.tracked[st.ContainerID]; !ok {
				r.tracked[st.ContainerID] = vid
				r.log.WithFields(logrus.Fields{"containerID": st.ContainerID, "volumeID": vid}).Info("RUTracker: now tracking container")
			}
		}
	}
	// remove stale ones
	for cid, vid := range r.tracked {
		if st, exists := states[vid]; !exists || st.ContainerID != cid || st.Frozen || st.RULimit <= 0 {
			delete(r.tracked, cid)
			r.log.WithFields(logrus.Fields{"containerID": cid, "volumeID": vid}).Info("RUTracker: stopped tracking container")
		}
	}
}

// sampleAll runs a sampling iteration over tracked containers
func (r *RUTracker) sampleAll(interval time.Duration) {
	for cid, vid := range r.tracked {
		ctx := context.Background()
		stats, err := r.monitor.GetContainerStats(ctx, cid)
		if err != nil {
			r.log.WithFields(logrus.Fields{"container": cid, "volume": vid}).WithError(err).Warn("RUTracker: failed to get container stats")
			continue
		}

		// Determine time delta: use interval as best-effort
		deltaHours := interval.Hours()

		// Memory in GB
		memGB := float64(stats.MemoryUsed) / float64(1<<30)

		// CPU equivalent in GB
		cpuGB := (stats.CPUPercent / 100.0) * defaultCPUToGB

		// Disk usage (use volume size captured from container manager state)
		var diskGB float64
		if st := r.containerManager.GetState(vid); st != nil {
			diskGB = st.LastSample.VolumeSizeM / 1024.0 // MiB -> GiB
		}

		// Equivalent GB for RU calculation
		equivGB := memGB + cpuGB + diskGB*defaultDiskWeight

		// RU units for this interval
		ruDelta := equivGB * deltaHours

		// Add RU to container state via manager helper (persisting and freeze-check handled there)
		if err := r.containerManager.AddRU(cid, ruDelta, time.Now(), stats.MemoryUsed, stats.CPUPercent); err != nil {
			r.log.WithFields(logrus.Fields{"container": cid, "volume": vid}).WithError(err).Warn("RUTracker: failed to add RU")
		}
	}
}

func (r *RUTracker) lock() {
	r.mu <- struct{}{}
}
func (r *RUTracker) unlock() {
	<-r.mu
}
