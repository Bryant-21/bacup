Function Fragment_Phase_02_End()
    W05_MQR_205P_QuestScript owningQuest = GetOwningQuest() as W05_MQR_205P_QuestScript
    If owningQuest == None || !owningQuest.IsRunning() || !owningQuest.IsStageDone(400) || owningQuest.IsStageDone(500)
        Return
    EndIf
    If owningQuest.SecurityRoomDoor != None
        ObjectReference doorRef = owningQuest.SecurityRoomDoor.GetReference()
        If doorRef != None
            doorRef.SetOpen(False)
        EndIf
    EndIf
    If owningQuest.SecurityRoomCollision != None
        ObjectReference collisionRef = owningQuest.SecurityRoomCollision.GetReference()
        If collisionRef != None
            collisionRef.EnableNoWait()
        EndIf
    EndIf
EndFunction
