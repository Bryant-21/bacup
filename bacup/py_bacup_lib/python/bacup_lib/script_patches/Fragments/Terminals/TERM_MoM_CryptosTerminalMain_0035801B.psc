Function ClaimPromotion(Int aiQuestIndex, Int aiStage)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > aiQuestIndex
        Quest targetQuest = masterScript.MoMQuestList[aiQuestIndex].MoMQuest
        If targetQuest != None && !targetQuest.IsStageDone(aiStage)
            targetQuest.SetStage(aiStage)
        EndIf
    EndIf
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        ClaimPromotion(8, masterScript.CONST_MoM04_ClaimedPromotion)
    EndIf
EndFunction

Function Fragment_Terminal_11(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        ClaimPromotion(2, masterScript.CONST_MoM01_ClaimedPromotion)
    EndIf
EndFunction

Function Fragment_Terminal_12(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        ClaimPromotion(3, masterScript.CONST_MoM02_ClaimedPromotion)
    EndIf
EndFunction
