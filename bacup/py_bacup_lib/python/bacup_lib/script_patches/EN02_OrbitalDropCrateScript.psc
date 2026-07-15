Event OnLoad()
    Quest mainQuest = GetMainQuest()
    If mainQuest != None && mainQuest.IsStageDone(340) && !mainQuest.IsStageDone(350)
        mainQuest.SetStage(350)
    EndIf
EndEvent

Event OnItemRemoved(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    If akDestContainer != Game.GetPlayer()
        Return
    EndIf
    Quest mainQuest = GetMainQuest()
    If mainQuest == None
        Return
    EndIf
    If akBaseItem == Game.GetFormFromFile(0x00052213, "SeventySix.esm") && !mainQuest.IsStageDone(358)
        mainQuest.SetStage(358)
    ElseIf akBaseItem == Game.GetFormFromFile(0x0029CC0F, "SeventySix.esm") && !mainQuest.IsStageDone(359)
        mainQuest.SetStage(359)
    EndIf
    If mainQuest.IsStageDone(358) && mainQuest.IsStageDone(359) && !mainQuest.IsStageDone(360)
        mainQuest.SetStage(360)
    EndIf
EndEvent

Quest Function GetMainQuest()
    Return Game.GetFormFromFile(0x000293A3, "SeventySix.esm") as Quest
EndFunction
