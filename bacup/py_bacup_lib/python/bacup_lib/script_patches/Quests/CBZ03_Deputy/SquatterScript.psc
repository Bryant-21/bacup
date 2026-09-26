Int Function B21LivingSquatters()
    Int living = 0
    Int index = 0
    While index < GetCount()
        Actor member = GetAt(index) as Actor
        If member != None && !member.IsDead()
            living += 1
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction

Bool Function B21ClearObjectiveIsLive()
    Quest owner = GetOwningQuest()
    If owner == None
        Return False
    EndIf
    Return owner.IsRunning() && owner.IsStageDone(100) && !owner.IsStageDone(200)
EndFunction

Event OnAliasInit()
    CancelTimer(1165322)
    StartTimer(15.0, 1165321)
EndEvent

Event OnAliasReset()
    CancelTimer(1165322)
    StartTimer(15.0, 1165321)
EndEvent

Event OnAliasShutdown()
    CancelTimer(1165321)
    CancelTimer(1165322)
EndEvent

Event OnTimer(Int aiTimerID)
    If !B21ClearObjectiveIsLive()
        Quest owner = GetOwningQuest()
        If owner != None && owner.IsRunning() && !owner.IsStageDone(200)
            StartTimer(15.0, 1165321)
        EndIf
        Return
    EndIf

    If B21LivingSquatters() > 0
        StartTimer(15.0, 1165321)
        Return
    EndIf

    If aiTimerID == 1165321
        ; Confirm after a grace period: the boss refs can read as empty while the radiant
        ; location is still streaming in.
        StartTimer(120.0, 1165322)
        Return
    EndIf

    ; FO76's server repopulated the radiant location for every run of this daily. Here an
    ; already-cleared location leaves DefaultCollectionAliasOnDeath with nothing left to
    ; report, so the only objective, the completion and the Stop() never happen.
    GetOwningQuest().SetStage(200)
EndEvent
