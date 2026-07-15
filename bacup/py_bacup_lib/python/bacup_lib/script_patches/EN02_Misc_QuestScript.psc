Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && currentPlayer.GetRef() != playerRef
        currentPlayer.ForceRefTo(playerRef)
    EndIf
    If !IsStageDone(10)
        SetStage(10)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    Int completionStage = iCompletionStage
    If completionStage <= 0
        completionStage = 100
    EndIf
    If auiStageID == completionStage
        CompleteQuest()
        Stop()
    EndIf
EndEvent
