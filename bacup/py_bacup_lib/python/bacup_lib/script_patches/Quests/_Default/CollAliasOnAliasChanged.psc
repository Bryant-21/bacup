; FO4 has no OnAliasChanged: references the respawn script adds are found by a short scan, and each living one
; receives ItemToAdd once.
Event OnAliasInit()
    StartTimer(2.0, 1)
EndEvent

Event OnAliasShutdown()
    CancelTimer(1)
EndEvent

Event OnTimer(Int aiTimerID)
    Quest owner = GetOwningQuest()
    If aiTimerID != 1 || owner == None || !owner.IsRunning() || ItemToAdd == None
        Return
    EndIf
    Int index = 0
    While index < GetCount()
        Actor target = GetAt(index) as Actor
        If target != None && !target.IsDead() && target.GetItemCount(ItemToAdd) == 0
            target.AddItem(ItemToAdd, NumToAdd)
        EndIf
        index += 1
    EndWhile
    StartTimer(2.0, 1)
EndEvent
