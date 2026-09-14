Function Fragment_Stage_0400_Item_00()
    If MTNZ05_Messenger_DeliveryScene != None && !MTNZ05_Messenger_DeliveryScene.IsPlaying()
        MTNZ05_Messenger_DeliveryScene.Start()
    EndIf
EndFunction
