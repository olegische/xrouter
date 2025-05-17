package controller

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"github.com/gin-gonic/gin"
	"io"
	"log"
	"net/http"
	"one-api/common"
	"one-api/dto"
	"one-api/model"
	"one-api/service"
	"one-api/setting"
	"strconv"
	"time"
)

func UpdateMidjourneyTaskBulk() {
	//imageModel := "midjourney"
	ctx := context.TODO()
	for {
		time.Sleep(time.Duration(15) * time.Second)

		tasks := model.GetAllUnFinishTasks()
		if len(tasks) == 0 {
			continue
		}

		common.LogInfo(ctx, fmt.Sprintf("Detected number of incomplete tasks: %v", len(tasks)))
		taskChannelM := make(map[int][]string)
		taskM := make(map[string]*model.Midjourney)
		nullTaskIds := make([]int, 0)
		for _, task := range tasks {
			if task.MjId == "" {
				// Count failed incomplete tasks
				nullTaskIds = append(nullTaskIds, task.Id)
				continue
			}
			taskM[task.MjId] = task
			taskChannelM[task.ChannelId] = append(taskChannelM[task.ChannelId], task.MjId)
		}
		if len(nullTaskIds) > 0 {
			err := model.MjBulkUpdateByTaskIds(nullTaskIds, map[string]any{
				"status":   "FAILURE",
				"progress": "100%",
			})
			if err != nil {
				common.LogError(ctx, fmt.Sprintf("Fix null mj_id task error: %v", err))
			} else {
				common.LogInfo(ctx, fmt.Sprintf("Fix null mj_id task success: %v", nullTaskIds))
			}
		}
		if len(taskChannelM) == 0 {
			continue
		}

		for channelId, taskIds := range taskChannelM {
			common.LogInfo(ctx, fmt.Sprintf("Channel #%d has incomplete tasks: %d", channelId, len(taskIds)))
			if len(taskIds) == 0 {
				continue
			}
			midjourneyChannel, err := model.CacheGetChannel(channelId)
			if err != nil {
				common.LogError(ctx, fmt.Sprintf("CacheGetChannel: %v", err))
				err := model.MjBulkUpdate(taskIds, map[string]any{
					"fail_reason": fmt.Sprintf("Failed to get channel information, please contact administrator, Channel ID: %d", channelId),
					"status":      "FAILURE",
					"progress":    "100%",
				})
				if err != nil {
					common.LogInfo(ctx, fmt.Sprintf("UpdateMidjourneyTask error: %v", err))
				}
				continue
			}
			requestUrl := fmt.Sprintf("%s/mj/task/list-by-condition", *midjourneyChannel.BaseURL)

			body, _ := json.Marshal(map[string]any{
				"ids": taskIds,
			})
			req, err := http.NewRequest("POST", requestUrl, bytes.NewBuffer(body))
			if err != nil {
				common.LogError(ctx, fmt.Sprintf("Get Task error: %v", err))
				continue
			}
			// Set timeout
			timeout := time.Second * 15
			ctx, cancel := context.WithTimeout(context.Background(), timeout)
			// Create a new request with a timeout context
			req = req.WithContext(ctx)
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set("mj-api-secret", midjourneyChannel.Key)
			resp, err := service.GetHttpClient().Do(req)
			if err != nil {
				common.LogError(ctx, fmt.Sprintf("Get Task Do req error: %v", err))
				continue
			}
			if resp.StatusCode != http.StatusOK {
				common.LogError(ctx, fmt.Sprintf("Get Task status code: %d", resp.StatusCode))
				continue
			}
			responseBody, err := io.ReadAll(resp.Body)
			if err != nil {
				common.LogError(ctx, fmt.Sprintf("Get Task parse body error: %v", err))
				continue
			}
			var responseItems []dto.MidjourneyDto
			err = json.Unmarshal(responseBody, &responseItems)
			if err != nil {
				common.LogError(ctx, fmt.Sprintf("Get Task parse body error2: %v, body: %s", err, string(responseBody)))
				continue
			}
			resp.Body.Close()
			req.Body.Close()
			cancel()

			for _, responseItem := range responseItems {
				task := taskM[responseItem.MjId]

				useTime := (time.Now().UnixNano() / int64(time.Millisecond)) - task.SubmitTime
				// If time exceeds one hour and progress is not 100%, consider the task failed
				if useTime > 3600000 && task.Progress != "100%" {
					responseItem.FailReason = "Upstream task timeout (over 1 hour)"
					responseItem.Status = "FAILURE"
				}
				if !checkMjTaskNeedUpdate(task, responseItem) {
					continue
				}
				task.Code = 1
				task.Progress = responseItem.Progress
				task.PromptEn = responseItem.PromptEn
				task.State = responseItem.State
				task.SubmitTime = responseItem.SubmitTime
				task.StartTime = responseItem.StartTime
				task.FinishTime = responseItem.FinishTime
				task.ImageUrl = responseItem.ImageUrl
				task.Status = responseItem.Status
				task.FailReason = responseItem.FailReason
				if responseItem.Properties != nil {
					propertiesStr, _ := json.Marshal(responseItem.Properties)
					task.Properties = string(propertiesStr)
				}
				if responseItem.Buttons != nil {
					buttonStr, _ := json.Marshal(responseItem.Buttons)
					task.Buttons = string(buttonStr)
				}
				shouldReturnQuota := false
				if (task.Progress != "100%" && responseItem.FailReason != "") || (task.Progress == "100%" && task.Status == "FAILURE") {
					common.LogInfo(ctx, task.MjId+" Build failed, "+task.FailReason)
					task.Progress = "100%"
					if task.Quota != 0 {
						shouldReturnQuota = true
					}
				}
				err = task.Update()
				if err != nil {
					common.LogError(ctx, "UpdateMidjourneyTask task error: "+err.Error())
				} else {
					if shouldReturnQuota {
						err = model.IncreaseUserQuota(task.UserId, task.Quota, false)
						if err != nil {
							common.LogError(ctx, "fail to increase user quota: "+err.Error())
						}
						logContent := fmt.Sprintf("Image generation failed %s, compensation %s", task.MjId, common.LogQuota(task.Quota))
						model.RecordLog(task.UserId, model.LogTypeSystem, logContent)
					}
				}
			}
		}
	}
}

func checkMjTaskNeedUpdate(oldTask *model.Midjourney, newTask dto.MidjourneyDto) bool {
	if oldTask.Code != 1 {
		return true
	}
	if oldTask.Progress != newTask.Progress {
		return true
	}
	if oldTask.PromptEn != newTask.PromptEn {
		return true
	}
	if oldTask.State != newTask.State {
		return true
	}
	if oldTask.SubmitTime != newTask.SubmitTime {
		return true
	}
	if oldTask.StartTime != newTask.StartTime {
		return true
	}
	if oldTask.FinishTime != newTask.FinishTime {
		return true
	}
	if oldTask.ImageUrl != newTask.ImageUrl {
		return true
	}
	if oldTask.Status != newTask.Status {
		return true
	}
	if oldTask.FailReason != newTask.FailReason {
		return true
	}
	if oldTask.FinishTime != newTask.FinishTime {
		return true
	}
	if oldTask.Progress != "100%" && newTask.FailReason != "" {
		return true
	}

	return false
}

func GetAllMidjourney(c *gin.Context) {
	p, _ := strconv.Atoi(c.Query("p"))
	if p < 0 {
		p = 0
	}

	// Parse other query parameters
	queryParams := model.TaskQueryParams{
		ChannelID:      c.Query("channel_id"),
		MjID:           c.Query("mj_id"),
		StartTimestamp: c.Query("start_timestamp"),
		EndTimestamp:   c.Query("end_timestamp"),
	}

	logs := model.GetAllTasks(p*common.ItemsPerPage, common.ItemsPerPage, queryParams)
	if logs == nil {
		logs = make([]*model.Midjourney, 0)
	}
	if setting.MjForwardUrlEnabled {
		for i, midjourney := range logs {
			midjourney.ImageUrl = setting.ServerAddress + "/mj/image/" + midjourney.MjId
			logs[i] = midjourney
		}
	}
	c.JSON(200, gin.H{
		"success": true,
		"message": "",
		"data":    logs,
	})
}

func GetUserMidjourney(c *gin.Context) {
	p, _ := strconv.Atoi(c.Query("p"))
	if p < 0 {
		p = 0
	}

	userId := c.GetInt("id")
	log.Printf("userId = %d \n", userId)

	queryParams := model.TaskQueryParams{
		MjID:           c.Query("mj_id"),
		StartTimestamp: c.Query("start_timestamp"),
		EndTimestamp:   c.Query("end_timestamp"),
	}

	logs := model.GetAllUserTask(userId, p*common.ItemsPerPage, common.ItemsPerPage, queryParams)
	if logs == nil {
		logs = make([]*model.Midjourney, 0)
	}
	if setting.MjForwardUrlEnabled {
		for i, midjourney := range logs {
			midjourney.ImageUrl = setting.ServerAddress + "/mj/image/" + midjourney.MjId
			logs[i] = midjourney
		}
	}
	c.JSON(200, gin.H{
		"success": true,
		"message": "",
		"data":    logs,
	})
}
