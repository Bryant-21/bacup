Actor Function SteelheartActor()
    If Steelheart == None
        Return None
    EndIf
    Return Steelheart.GetActorReference()
EndFunction

Function WatchSteelheart()
    Actor steelheartRef = SteelheartActor()
    If steelheartRef == None
        Return
    EndIf
    RegisterForRemoteEvent(steelheartRef, "OnEnterBleedout")
    RegisterForRemoteEvent(steelheartRef, "OnDying")
EndFunction

Bool Function PatrolStillRunning()
    If !IsRunning()
        Return False
    EndIf
    Return !IsStageDone(5000) && !IsStageDone(5500) && !IsStageDone(6000)
EndFunction

Function FailPatrol()
    If !PatrolStillRunning()
        Return
    EndIf
    SetStage(5500)
EndFunction

Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    WatchSteelheart()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 200
        WatchSteelheart()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer() && PatrolStillRunning()
        WatchSteelheart()
    EndIf
EndEvent

Event Actor.OnEnterBleedout(Actor akSender)
    If akSender == SteelheartActor()
        FailPatrol()
    EndIf
EndEvent

Event Actor.OnDying(Actor akSender, Actor akKiller)
    If akSender == SteelheartActor()
        FailPatrol()
    EndIf
EndEvent

Event OnQuestShutdown()
    UnregisterForAllEvents()
EndEvent
