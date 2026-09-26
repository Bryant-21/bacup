; A waiter on break agrees to report for duty.
Function Fragment_Begin(ObjectReference akSpeakerRef)
    MTNM04QuestScript questScript = GetOwningQuest() as MTNM04QuestScript
    If questScript != None
        questScript.SendRobotToWork(akSpeakerRef as Actor)
    EndIf
EndFunction
