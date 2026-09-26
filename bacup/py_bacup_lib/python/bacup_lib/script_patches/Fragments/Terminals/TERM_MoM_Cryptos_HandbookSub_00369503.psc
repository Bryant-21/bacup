Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 2
        Quest initiateQuest = masterScript.MoMQuestList[2].MoMQuest
        If initiateQuest != None && initiateQuest.IsRunning() && initiateQuest.IsStageDone(80) && !initiateQuest.IsStageDone(85)
            initiateQuest.SetStage(85)
        EndIf
    EndIf
EndFunction
