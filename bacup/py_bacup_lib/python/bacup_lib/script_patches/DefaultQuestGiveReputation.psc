Event OnQuestInit()
    B21ReputationGranted = False
EndEvent

; Reputation is paid once per successful run to a participating player.
Event OnStageSet(Int auiStageID, Int auiItemID)
    If B21ReputationGranted || !IsCompleted()
        Return
    EndIf
    Quest owner = Self as Quest
    DefaultEventQuest eventQuest = owner as DefaultEventQuest
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || ReputationAV == None || RepValue == None
        Return
    EndIf
    B21ReputationGranted = True
    If eventQuest != None && !eventQuest.IsPlayerParticipating()
        Return
    EndIf
    playerRef.ModValue(ReputationAV, RepValue.GetValue())
EndEvent
