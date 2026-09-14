; Stage 100 is the pointer's only RunOnStart stage, note "Investigate cabin". Objective 100
; "Investigate the Cabin" is the record's only objective and targets alias 2 HolotapeLocation
; (forced reference 3EBB97). Displaying it is the stage's whole contract; the root
; GQ_MiscRegionPointerScript also displays it from OnQuestInit, so this must stay idempotent.
Function Fragment_Stage_0100_Item_00()
    If !IsObjectiveCompleted(100)
        SetObjectiveDisplayed(100)
    EndIf
EndFunction

; Stage 1000 carries the CompleteQuest flag, which completes the quest and awards
; QuestCompletionXP 098952 natively. The fragment therefore closes the objective only —
; it must not complete or stop the quest again.
Function Fragment_Stage_1000_Item_00()
    If !IsObjectiveCompleted(100)
        SetObjectiveCompleted(100)
    EndIf
EndFunction
