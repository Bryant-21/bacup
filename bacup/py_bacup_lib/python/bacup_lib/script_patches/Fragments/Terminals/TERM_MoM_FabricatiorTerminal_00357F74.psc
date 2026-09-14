Function CompleteFabrication(Int aiQuestIndex, Int aiStage, Bool abCloth)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > aiQuestIndex
        Quest targetQuest = masterScript.MoMQuestList[aiQuestIndex].MoMQuest
        If targetQuest != None && !targetQuest.IsStageDone(aiStage)
            If MoMFabricatorActivatorRef != None
                If abCloth
                    MoMFabricatorActivatorRef.ClientFabricateCloth()
                Else
                    MoMFabricatorActivatorRef.ClientFabricateMetal()
                EndIf
            EndIf
            targetQuest.SetStage(aiStage)
        EndIf
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        CompleteFabrication(4, masterScript.CONST_MoM02A_FiledReport, false)
    EndIf
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        CompleteFabrication(5, masterScript.CONST_MoM02B_FabricatedItem, false)
    EndIf
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        CompleteFabrication(6, masterScript.CONST_MoM02C_FabricatedItem, false)
    EndIf
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        CompleteFabrication(5, masterScript.CONST_MoM02B_AcquiredSwingAnalyzer, false)
    EndIf
EndFunction

Function Fragment_Terminal_08(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 9
        Quest targetQuest = masterScript.MoMQuestList[9].MoMQuest
        Int stage = masterScript.CONST_MoMVeil_UseTheFabricatorAndCheckpoint
        If targetQuest != None && !targetQuest.IsStageDone(stage)
            Game.GetPlayer().RemoveItem(MoM_ClothesMistressOfMysteryVeilCorpse, 1, true)
            CompleteFabrication(9, stage, true)
        EndIf
    EndIf
EndFunction

Function Fragment_Terminal_09(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        CompleteFabrication(10, masterScript.CONST_MoMDress_Completed, true)
    EndIf
EndFunction
