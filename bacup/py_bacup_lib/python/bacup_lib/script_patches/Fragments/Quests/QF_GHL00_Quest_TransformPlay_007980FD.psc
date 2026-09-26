Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Game.GetPlayer()
    Alias_Player.ForceRefIfEmpty(playerRef)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(25)
    SetObjectiveCompleted(30)
EndFunction

Function Fragment_Stage_9900_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && GHL00_Quest_DisguisePointer_StartKeyword != None
        GHL00_Quest_DisguisePointer_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
    Stop()
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    If GHL00_Quest_TransformPlayer_GhoulInfo != None
        GHL00_Quest_TransformPlayer_GhoulInfo.Show()
    EndIf
EndFunction
