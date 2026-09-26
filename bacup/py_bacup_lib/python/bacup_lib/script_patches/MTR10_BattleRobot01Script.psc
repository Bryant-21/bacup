Event OnAliasInit()
    BeginHealthWatch()
EndEvent

Event OnAliasReset()
    BeginHealthWatch()
EndEvent

Event OnAliasShutdown()
    CancelTimer(3311)
EndEvent

Function BeginHealthWatch()
    CancelTimer(3311)
    CurrentHealth = 0.0
    FullHealth = 0.0
    ObjectReference robotRef = GetReference()
    If robotRef == None || HealthAV == None
        Return
    EndIf
    FullHealth = robotRef.GetBaseValue(HealthAV)
    CurrentHealth = robotRef.GetValue(HealthAV)
    StartTimer(2.0, 3311)
EndFunction

Function RefreshHealth(ObjectReference akRobotRef)
    If akRobotRef == None || HealthAV == None
        Return
    EndIf
    CurrentHealth = akRobotRef.GetValue(HealthAV)
    If FullHealth <= 0.0
        FullHealth = akRobotRef.GetBaseValue(HealthAV)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 3311
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    ObjectReference robotRef = GetReference()
    If owningQuest == None || robotRef == None || !owningQuest.IsRunning()
        Return
    EndIf
    ; The retreat is a one-shot, and DefaultAliasOnDeath owns the killed-early path (150).
    If owningQuest.GetStageDone(100) || owningQuest.GetStageDone(150) || owningQuest.GetStageDone(160)
        Return
    EndIf
    RefreshHealth(robotRef)
    Actor robotActor = robotRef as Actor
    If robotActor == None || robotActor.IsDead()
        Return
    EndIf
    ; FO76 bound no damage threshold, so the bot breaks off once it has lost most of its
    ; health; stage 100 hands it to MTR10_Battle_Robot01RETREAT and its damage-resist spell.
    If owningQuest.GetStageDone(10) && FullHealth > 0.0 && CurrentHealth <= FullHealth * 0.4
        owningQuest.SetStage(100)
        Return
    EndIf
    StartTimer(2.0, 3311)
EndEvent
