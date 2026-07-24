Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget.HasKeyword(InitialQuestStartKeyword)
        DenizenDialogueScript ownerQuest = InitialQuest as DenizenDialogueScript
        If ownerQuest
            DenizenDialogueScript:AliasStruct[] structs = ownerQuest.ArrayofAliasStructs as DenizenDialogueScript:AliasStruct[]
            int found = structs.FindStruct("TargetActor", akTarget.GetActorBase())
            If found >= 0
                If structs[found].DestAlias.GetReference() != akTarget as ObjectReference
                    structs[found].DestAlias.ForceRefTo(akTarget)
                EndIf
            EndIf
        EndIf
    EndIf
EndEvent
