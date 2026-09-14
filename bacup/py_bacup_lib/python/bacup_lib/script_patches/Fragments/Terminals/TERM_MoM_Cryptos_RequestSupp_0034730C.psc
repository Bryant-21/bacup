Function RecordDatabaseQuery(Int aiQuestIndex, Int aiStage)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > aiQuestIndex
        Quest targetQuest = masterScript.MoMQuestList[aiQuestIndex].MoMQuest
        If targetQuest != None && !targetQuest.IsStageDone(aiStage)
            targetQuest.SetStage(aiStage)
        EndIf
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        RecordDatabaseQuery(2, masterScript.CONST_MoM01_RequestedMentorAssignment)
    EndIf
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        RecordDatabaseQuery(4, masterScript.CONST_MoM02A_SearchedForTarget)
    EndIf
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        RecordDatabaseQuery(5, masterScript.CONST_MoM02B_SearchedForTarget)
    EndIf
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        RecordDatabaseQuery(6, masterScript.CONST_MoM02C_SearchedForTarget)
    EndIf
EndFunction
