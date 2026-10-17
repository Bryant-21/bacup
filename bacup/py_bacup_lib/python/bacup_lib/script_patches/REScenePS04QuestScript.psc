; RE_TravelPS01 rogue Sentry Bot. Its hello line greets the player as Commander (100) and the greet scene makes it follow
; (200: SentryAlly/SentryAllyFollow, Sentry Bot Faction shared with CommanderPlayer). It turns hostile (SentryEnemyStage
; 300, set by each scene's completion) when the player attacks it (250, CombatScene), strays past
; SentryGreaterThanDistance for AbandonTimerLength seconds (AbandonScene), or after EncounterEndTimerLength seconds of
; escort duty (EnemyScene, "Invalid Commander identity detected").
Event OnQuestInit()
    CommanderPlayer.ForceRefIfEmpty(Game.GetPlayer())
    ObjectReference sentryRef = Sentry.GetReference()
    If sentryRef != None && sentryRef.IsDisabled()
        sentryRef.Enable()
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    Actor sentryActor = Sentry.GetActorReference()
    If auiStageID == 200
        If sentryActor != None && !IsStageDone(SentryEnemyStage)
            SentryAlly.ForceRefTo(sentryActor)
            SentryAllyFollow.ForceRefTo(sentryActor)
            StartTimer(EncounterEndTimerLength as Float, EncounterEndTimerID)
            StartTimer(2.0, FollowTimerID)
        EndIf
    ElseIf auiStageID == 250
        If !IsStageDone(SentryEnemyStage)
            StartTurnScene(RE_ScenePS04_CombatScene)
        EndIf
    ElseIf auiStageID == SentryEnemyStage
        CancelTimer(EncounterEndTimerID)
        CancelTimer(FollowTimerID)
        CancelTimer(AbandonTimerID)
        SentryAlly.Clear()
        SentryAllyFollow.Clear()
        CommanderPlayer.Clear()
        If sentryActor != None && !sentryActor.IsDead()
            SentryEnemy.ForceRefTo(sentryActor)
            sentryActor.StartCombat(Game.GetPlayer())
        EndIf
    ElseIf auiStageID == 400
        CancelTimer(EncounterEndTimerID)
        CancelTimer(FollowTimerID)
        CancelTimer(AbandonTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If IsStageDone(SentryEnemyStage) || IsStageDone(400)
        Return
    EndIf
    If aiTimerID == EncounterEndTimerID
        CancelTimer(FollowTimerID)
        CancelTimer(AbandonTimerID)
        StartTurnScene(RE_ScenePS04_EnemyScene)
    ElseIf aiTimerID == FollowTimerID
        If SentryOutOfRange()
            StartTimer(AbandonTimerLength as Float, AbandonTimerID)
        Else
            StartTimer(2.0, FollowTimerID)
        EndIf
    ElseIf aiTimerID == AbandonTimerID
        If SentryOutOfRange()
            CancelTimer(EncounterEndTimerID)
            StartTurnScene(RE_ScenePS04_AbandonScene)
        Else
            StartTimer(2.0, FollowTimerID)
        EndIf
    EndIf
EndEvent

Bool Function SentryOutOfRange()
    ObjectReference sentryRef = Sentry.GetReference()
    Return sentryRef != None && sentryRef.GetDistance(Game.GetPlayer()) > SentryGreaterThanDistance
EndFunction

; Each turn scene sets SentryEnemyStage on completion; an unloaded sentry cannot play it, so it turns at once.
Function StartTurnScene(Scene akTurnScene)
    ObjectReference sentryRef = Sentry.GetReference()
    If akTurnScene != None && sentryRef != None && sentryRef.Is3DLoaded()
        If !akTurnScene.IsPlaying()
            akTurnScene.Start()
        EndIf
    ElseIf !IsStageDone(SentryEnemyStage)
        SetStage(SentryEnemyStage)
    EndIf
EndFunction
