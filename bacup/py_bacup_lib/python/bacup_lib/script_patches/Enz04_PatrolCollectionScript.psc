Event OnAliasInit()
    bCleanUpCollection = False
EndEvent

Function CleanupPatrol(Bool abRetireLoadedBots = False)
    If !bCleanUpCollection
        Return
    EndIf
    Int index = GetCount() - 1
    While index >= 0
        Actor patrolBot = GetAt(index) as Actor
        If patrolBot != None && patrolBot != Game.GetPlayer()
            If !patrolBot.Is3DLoaded()
                RemoveRef(patrolBot)
                patrolBot.DisableNoWait()
                patrolBot.Delete()
            ElseIf abRetireLoadedBots && !patrolBot.IsDead()
                patrolBot.Kill()
            EndIf
        EndIf
        index -= 1
    EndWhile
    If GetCount() == 0 && PatrolCenterMarkers != None && PatrolCenterMarker != None
        PatrolCenterMarkers.RemoveRef(PatrolCenterMarker)
    EndIf
EndFunction

Event OnUnload(ObjectReference akSenderRef)
    If bCleanUpCollection && akSenderRef != None && Find(akSenderRef) >= 0
        CleanupPatrol(False)
    EndIf
EndEvent

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    If akSenderRef != None && Find(akSenderRef) >= 0
        CleanupPatrol(False)
    EndIf
EndEvent

Event OnAliasShutdown()
    Int index = GetCount() - 1
    While index >= 0
        ObjectReference patrolBot = GetAt(index)
        If patrolBot != None && patrolBot != Game.GetPlayer()
            RemoveRef(patrolBot)
            patrolBot.DisableNoWait()
            patrolBot.Delete()
        EndIf
        index -= 1
    EndWhile
    If PatrolCenterMarkers != None && PatrolCenterMarker != None
        PatrolCenterMarkers.RemoveRef(PatrolCenterMarker)
    EndIf
    bCleanUpCollection = False
EndEvent
