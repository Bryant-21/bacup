; Restore child ABI shared with COMP_RQ_Restore_Fetch_Script: once the saved stage has reached
; RestoreStage_ClearTargetActor, the target actor aliases are cleared so the restored quest re-fills them.
Function RestoreQuestCustom(Int restoreQuestStage)
    If restoreQuestStage < RestoreStage_ClearTargetActor || TargetAliasesToClear == None
        Return
    EndIf
    Int index = 0
    While index < TargetAliasesToClear.Length
        If TargetAliasesToClear[index] != None
            TargetAliasesToClear[index].Clear()
        EndIf
        index += 1
    EndWhile
EndFunction
