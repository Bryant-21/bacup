Event OnQuestInit()
    Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
    COMP_RQ_Ins = QuestTarget as COMP_RQ_Script
    If !COMP_RQ_Ins
        Return
    EndIf

    CopyReferenceAlias(Alias_Player, COMP_RQ_Ins.Alias_Player)
    CopyReferenceAlias(Alias_Companion, COMP_RQ_Ins.Alias_Companion)
    CopyReferenceAlias(Alias_Object, COMP_RQ_Ins.Alias_Object)
    CopyReferenceAlias(Alias_ObjectMarker, COMP_RQ_Ins.Alias_ObjectMarker)
    CopyReferenceAlias(Alias_ObjectContainer, COMP_RQ_Ins.Alias_ObjectContainer)
    CopyReferenceAlias(Alias_TargetActor, COMP_RQ_Ins.Alias_TargetActor)
    If Alias_Location && Alias_Location.GetLocation() && COMP_RQ_Ins.Alias_Location
        COMP_RQ_Ins.Alias_Location.ForceLocationTo(Alias_Location.GetLocation())
    EndIf

    If StoryEventKeyword
        StoryEventKeyword.SendStoryEventAndWait(Alias_Location.GetLocation(), Alias_Companion.GetReference(), Alias_Object.GetReference())
    EndIf
    Stop()
EndEvent

Function CopyReferenceAlias(ReferenceAlias sourceAlias, ReferenceAlias targetAlias)
    If sourceAlias && targetAlias && sourceAlias.GetReference()
        targetAlias.ForceRefTo(sourceAlias.GetReference())
    EndIf
EndFunction
