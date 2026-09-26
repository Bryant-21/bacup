Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    Quest owner = GetTerminalOwnerQuest()
    If owner == None || StageSet == None || !owner.IsRunning()
        Return
    EndIf

    Int index = 0
    While index < StageSet.Length
        StageData entry = StageSet[index]
        If entry.iMenuItemTarget == auiMenuItemID && entry.iStageToSet > 0
            Bool prereqMet = entry.iPrereqStage <= 0 || owner.IsStageDone(entry.iPrereqStage)
            Bool notFinished = entry.iCompletionStage <= 0 || !owner.IsStageDone(entry.iCompletionStage)
            If prereqMet && notFinished
                owner.SetStage(entry.iStageToSet)
            EndIf
        EndIf
        index += 1
    EndWhile
EndEvent

; FO76 terminals carry the owning quest on the TERM record itself; Fallout 4's
; TERM has no such field, so the conversion binds this script without one.
; Resolve the owner from the terminal's own FormID instead. Add a branch here
; when another converted quest starts using this script.
Quest Function GetTerminalOwnerQuest()
    Quest trainingDay = Game.GetFormFromFile(0x0067C1E9, "SeventySix.esm") as Quest
    If trainingDay == None
        Return None
    EndIf

    Int pluginOffset = trainingDay.GetFormID() - 0x0067C1E9
    Int localId = Self.GetFormID() - pluginOffset
    If (localId >= 0x0067C27A && localId <= 0x0067C2A0) || (localId >= 0x0068C4E8 && localId <= 0x0068C516)
        Return trainingDay
    EndIf
    Return None
EndFunction
