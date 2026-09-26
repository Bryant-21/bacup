; FO76 DefaultEventQuest services (standard start stage, player join by
; proximity, per-player intro lines) ran on the server. They are restored here
; for the three Enclave events only. Sibling scripts that override OnQuestInit
; must call Parent.OnQuestInit().

Event OnQuestInit()
    ENEvent_BeginStandardEvent()
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    Actor playerRef = akObj1 as Actor
    If playerRef != None && playerRef == Game.GetPlayer()
        ENEvent_PlayerArrived(playerRef)
    EndIf
EndEvent

Function ENEvent_BeginStandardEvent()
    DefaultEventQuest eventDefaults = (Self as Quest) as DefaultEventQuest
    If eventDefaults == None
        Return
    EndIf
    If eventDefaults.EventStageStandard >= 0 && !IsStageDone(eventDefaults.EventStageStandard)
        SetStage(eventDefaults.EventStageStandard)
    EndIf
    ENEvent_WatchForPlayerArrival()
EndFunction

Function ENEvent_WatchForPlayerArrival()
    DefaultEventQuest eventDefaults = (Self as Quest) as DefaultEventQuest
    Actor playerRef = Game.GetPlayer()
    If eventDefaults == None || eventDefaults.CenterMarker == None || playerRef == None
        Return
    EndIf
    ObjectReference center = eventDefaults.CenterMarker.GetReference()
    If center == None
        Return
    EndIf
    Float enterRadius = eventDefaults.ActivityEnterRadius
    DefaultEventRadiusOverrideScript radiusOverride = center as DefaultEventRadiusOverrideScript
    If radiusOverride != None && radiusOverride.ActivityEnterRadius > 0.0
        enterRadius = radiusOverride.ActivityEnterRadius
    EndIf
    If playerRef.GetDistance(center) <= enterRadius
        ENEvent_PlayerArrived(playerRef)
    Else
        RegisterForDistanceLessThanEvent(playerRef, center, enterRadius)
    EndIf
EndFunction

Function ENEvent_PlayerArrived(Actor akPlayer)
    DefaultEventQuest eventDefaults = (Self as Quest) as DefaultEventQuest
    If akPlayer == None || eventDefaults == None || !IsRunning() || IsStopping()
        Return
    EndIf
    If eventDefaults.EventStageDisableLateJoin >= 0 && IsStageDone(eventDefaults.EventStageDisableLateJoin)
        Return
    EndIf
    If bEnclavePlayersOnly && EN02_JoinedEnclaveValue != None && akPlayer.GetValue(EN02_JoinedEnclaveValue) <= 0.0
        Return
    EndIf
    If eventDefaults.AliasEventPlayers != None && eventDefaults.AliasEventPlayers.Find(akPlayer) < 0
        eventDefaults.AliasEventPlayers.AddRef(akPlayer)
    EndIf
    If iFirstPlayerJoinedStage >= 0 && !IsStageDone(iFirstPlayerJoinedStage)
        SetStage(iFirstPlayerJoinedStage)
    EndIf
    If CurrentPlayers == None || CurrentPlayers.Find(akPlayer) >= 0
        Return
    EndIf
    CurrentPlayers.AddRef(akPlayer)
    Topic intro = NormalStartUpTopic
    If FirstTimePlayingValue != None && akPlayer.GetValue(FirstTimePlayingValue) <= 0.0
        akPlayer.SetValue(FirstTimePlayingValue, 1.0)
        If FirstTimeStartupTopic != None
            intro = FirstTimeStartupTopic
        EndIf
    EndIf
    ENEvent_SayToPlayer(intro)
EndFunction

Function ENEvent_SayToPlayer(Topic akTopic)
    If akTopic == None || MODUSRadioVoice == None
        Return
    EndIf
    ObjectReference voice = MODUSRadioVoice.GetReference()
    If voice != None
        voice.Say(akTopic, None, True)
    EndIf
EndFunction

Function ENEvent_RecordCompletion()
    Actor player = Game.GetPlayer()
    If player == None
        Return
    EndIf

    If CompletionTrackingValue != None
        player.SetValue(CompletionTrackingValue, player.GetValue(CompletionTrackingValue) + 1.0)
    EndIf

    If iCommendationValue <= 0 || EN05_MQ_Officer == None || !EN05_MQ_Officer.IsRunning()
        Return
    EndIf

    EN05_MQ_QuestScript officer = EN05_MQ_Officer as EN05_MQ_QuestScript
    If officer == None || !officer.IsStageDone(officer.iPlayerRegisteredStage) \
        || officer.IsStageDone(officer.iCommendationsCompletedStage)
        Return
    EndIf

    Int supportStage = officer.iCompletedBunkerPromotion
    If supportStage >= 0 && !officer.IsStageDone(supportStage)
        officer.SetObjectiveCompleted(supportStage)
        officer.SetStage(supportStage)
    EndIf

    officer.EN05MQ_AddCommendations(iCommendationValue)
EndFunction
