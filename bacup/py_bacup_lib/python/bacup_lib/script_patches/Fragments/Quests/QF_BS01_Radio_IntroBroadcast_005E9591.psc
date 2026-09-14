Function Fragment_Stage_0020_Item_00()
    If Alias_Player == None || Alias_Player.GetReference() != Game.GetPlayer()
        Return
    EndIf

    If BS01_MQ00_Misc_Breadcrumb != None && BS01_MQ00_Misc_Breadcrumb.IsRunning() && !BS01_MQ00_Misc_Breadcrumb.IsStageDone(200)
        BS01_MQ00_Misc_Breadcrumb.SetStage(200)
    EndIf

    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Stop()
EndFunction
