Function Fragment_Stage_0001_Item_00()
    If Alias_Player
        Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_1001_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && pRSVP04_Holotape_Patrol_Part1 && playerRef.GetItemCount(pRSVP04_Holotape_Patrol_Part1) == 0
        playerRef.AddItem(pRSVP04_Holotape_Patrol_Part1, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveCompleted(1000)
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveDisplayed(1900)
EndFunction

Function Fragment_Stage_1901_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && pRSVP04_Holotape_Patrol_Part2 && playerRef.GetItemCount(pRSVP04_Holotape_Patrol_Part2) == 0
        playerRef.AddItem(pRSVP04_Holotape_Patrol_Part2, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveCompleted(1900)
    SetObjectiveDisplayed(2000)
EndFunction

Function Fragment_Stage_2001_Item_00()
    ObjectReference containerRef = Alias_Container_02.GetReference()
    If containerRef && pRSVP04_Holotape_Patrol_Part2 && containerRef.GetItemCount(pRSVP04_Holotape_Patrol_Part2) == 0
        containerRef.AddItem(pRSVP04_Holotape_Patrol_Part2, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_2200_Item_00()
    SetObjectiveCompleted(2000)
    SetObjectiveDisplayed(2900)
EndFunction

Function Fragment_Stage_2800_Item_00()
    SetObjectiveCompleted(2900)
EndFunction

Function Fragment_Stage_2900_Item_00()
    SetObjectiveDisplayed(3000)
EndFunction

Function Fragment_Stage_2901_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && pRSVP04_Holotape_Patrol_Part3 && playerRef.GetItemCount(pRSVP04_Holotape_Patrol_Part3) == 0
        playerRef.AddItem(pRSVP04_Holotape_Patrol_Part3, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_3000_Item_00()
    SetObjectiveCompleted(3000)
    SetObjectiveDisplayed(3100)
EndFunction

Function Fragment_Stage_3001_Item_00()
    ObjectReference containerRef = Alias_Container_03.GetReference()
    If containerRef && pRSVP04_Holotape_Patrol_Part3 && containerRef.GetItemCount(pRSVP04_Holotape_Patrol_Part3) == 0
        containerRef.AddItem(pRSVP04_Holotape_Patrol_Part3, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_3100_Item_00()
    SetObjectiveCompleted(3100)
    SetObjectiveDisplayed(3200)
EndFunction

Function Fragment_Stage_3200_Item_00()
    SetObjectiveCompleted(3200)
    SetObjectiveDisplayed(3500)
EndFunction

Function Fragment_Stage_3800_Item_00()
    SetObjectiveCompleted(3500)
EndFunction

Function Fragment_Stage_3900_Item_00()
    SetObjectiveDisplayed(4000)
EndFunction

Function Fragment_Stage_3901_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && pRSVP04_Holotape_Patrol_Part4 && playerRef.GetItemCount(pRSVP04_Holotape_Patrol_Part4) == 0
        playerRef.AddItem(pRSVP04_Holotape_Patrol_Part4, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_4000_Item_00()
    SetObjectiveCompleted(4000)
    SetObjectiveDisplayed(4100)
EndFunction

Function Fragment_Stage_4001_Item_00()
    ObjectReference containerRef = Alias_Container_CharlieStation.GetReference()
    If containerRef && pRSVP04_Holotape_Patrol_Part4 && containerRef.GetItemCount(pRSVP04_Holotape_Patrol_Part4) == 0
        containerRef.AddItem(pRSVP04_Holotape_Patrol_Part4, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_4200_Item_00()
    SetObjectiveCompleted(4100)
    SetObjectiveDisplayed(4900)
EndFunction

Function Fragment_Stage_4800_Item_00()
    SetObjectiveCompleted(4900)
EndFunction

Function Fragment_Stage_4900_Item_00()
    SetObjectiveDisplayed(5000)
EndFunction

Function Fragment_Stage_5000_Item_00()
    SetObjectiveCompleted(5000)
    SetObjectiveDisplayed(5100)
EndFunction

Function Fragment_Stage_5100_Item_00()
    SetObjectiveCompleted(5100)
    SetObjectiveDisplayed(5200)
EndFunction

Function Fragment_Stage_5500_Item_00()
    SetObjectiveCompleted(5200)
    SetObjectiveDisplayed(5500)
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(5500)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP04_AV_QuestCompletion, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_9500_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_9999_Item_00()
    Stop()
EndFunction
