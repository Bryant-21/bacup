; FO76's on-connect node (72A2BF) also required GetLevel >= 25. That PCON node is not
; converted (this quest autostarts instead), so the level gate lives here; players below
; 25 are picked up later by the Storm_MQ01_Breadcrumb OnLevel node.
Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && playerRef.GetLevel() >= 25 && Storm_MQ01_Breadcrumb != None && Storm_MQ01_Breadcrumb_QuestStartKeyword != None
        If !Storm_MQ01_Breadcrumb.IsRunning() && !Storm_MQ01_Breadcrumb.IsCompleted()
            Storm_MQ01_Breadcrumb_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
    Stop()
EndFunction
