Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    Actor player = Game.GetPlayer()
    If player == None
        Return
    EndIf

    Int i = 0
    While i < MenuData.Length
        MenuDatum entry = MenuData[i]
        If entry.iMenuItemTarget == auiMenuItemID
            If entry.BlockingActorValue == None || player.GetValue(entry.BlockingActorValue) < entry.iBlockingValue
                If entry.ItemToAdd != None && entry.iItemAmount > 0
                    player.AddItem(entry.ItemToAdd, entry.iItemAmount, False)
                EndIf
                If entry.BlockingActorValue != None
                    player.SetValue(entry.BlockingActorValue, entry.iBlockingValue as Float)
                EndIf
            EndIf
            Return
        EndIf
        i += 1
    EndWhile
EndEvent
