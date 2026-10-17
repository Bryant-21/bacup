Event OnQuestInit()
    Actor playerRef = MyPlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        If WS01status != None
            playerRef.SetValue(WS01status, 0.0)
        EndIf
        RegisterForRemoteEvent(playerRef, "OnKill")
        UpdateKillCount(playerRef)
    EndIf
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndEvent

; The objective text shows <Variable=NumberKilled>/<Variable=KillsNeeded>, which FO76's server fills.
Function UpdateKillCount(Actor playerRef)
    B21:QuestVariables variables = (Self as Quest) as B21:QuestVariables
    If variables == None || playerRef == None || WS01status == None || WS01MaxKills == None
        Return
    EndIf
    variables.SetVariable("NumberKilled", playerRef.GetValue(WS01status))
    variables.SetVariable("KillsNeeded", WS01MaxKills.GetValue())
EndFunction

; WhitespringFaction holds only the resort's robots, so it can't filter the golfers; robot kills
; never reach the player's OnKill anyway.
Event Actor.OnKill(Actor akSender, Actor akVictim)
    Actor playerRef = akSender
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    ; Any kill refreshes the counter, so a save from before it was shown catches up.
    UpdateKillCount(playerRef)
    If akVictim == None || IsStageDone(StageToSet)
        Return
    EndIf
    If FeralGhoulGolfClothes != None && !akVictim.WornHasKeyword(FeralGhoulGolfClothes)
        Return
    EndIf
    Location victimLocation = akVictim.GetCurrentLocation()
    If LocationKeyword != None && (victimLocation == None || !victimLocation.HasKeyword(LocationKeyword))
        Return
    EndIf
    If playerRef == None || WS01status == None || WS01MaxKills == None
        Return
    EndIf

    playerRef.SetValue(WS01status, playerRef.GetValue(WS01status) + 1.0)
    UpdateKillCount(playerRef)
    If playerRef.GetValue(WS01status) >= WS01MaxKills.GetValue()
        SetStage(StageToSet)
    EndIf
EndEvent

; Stage 1000 ("Complete quest") carries the reward but no completion flag; FO76's server ends the quest.
Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == StageToSet
        SetObjectiveCompleted(ObjectiveToDisplay)
        CompleteQuest()
        Stop()
    EndIf
EndEvent

Event OnQuestShutdown()
    Actor playerRef = MyPlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnKill")
    EndIf
EndEvent
