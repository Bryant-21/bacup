; Progress ticks every ProgressTimerSeconds while the player uses one of the collection's
; furniture. A lone player using the only piece also earns AllBeingUsedBonus.
Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && CanContribute()
        StartTimer(ProgressTimerSeconds, 1)
    EndIf
EndEvent

ObjectReference Function FurnitureInUseByPlayer()
    Actor playerRef = Game.GetPlayer()
    If playerRef.GetSitState() < 2
        Return None
    EndIf
    Int index = 0
    While index < GetCount()
        ObjectReference candidate = GetAt(index)
        If candidate != None && candidate.IsFurnitureInUse() && playerRef.GetDistance(candidate) < 150.0
            Return candidate
        EndIf
        index += 1
    EndWhile
    Return None
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1 || !CanContribute()
        Return
    EndIf
    ObjectReference usedFurniture = FurnitureInUseByPlayer()
    If usedFurniture == None
        Return
    EndIf
    Float contributions = 1.0
    If AllBeingUsedBonus > 0.0 && GetCount() == 1 && ModPercentage != 0.0
        contributions += AllBeingUsedBonus / ModPercentage
    EndIf
    ContributeProgress(contributions, usedFurniture)
    StartTimer(ProgressTimerSeconds, 1)
EndEvent
