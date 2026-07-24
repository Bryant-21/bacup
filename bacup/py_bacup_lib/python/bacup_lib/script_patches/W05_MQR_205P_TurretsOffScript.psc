Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    W05_MQR_205P_QuestScript owningQuestScript = GetOwningQuest() as W05_MQR_205P_QuestScript
    If owningQuestScript != None && owningQuestScript.SecurityRoomTurrets != None
        owningQuestScript.SecurityRoomTurrets.DisableAll()
    EndIf
EndEvent
