Event OnInit()
    Actor player = Game.GetPlayer()
    If player == None
        Return
    EndIf

    Int i = 0
    Int count = PlayerDataList.Length
    While i < count
        If PlayerDataList[i].ActorValueToCheck != None
            If player.GetValue(PlayerDataList[i].ActorValueToCheck) == PlayerDataList[i].ValueToCheck as Float
                If PlayerDataList[i].FactionToAdd != None
                    player.AddToFaction(PlayerDataList[i].FactionToAdd)
                EndIf
                If PlayerDataList[i].FactionToRemove != None
                    player.RemoveFromFaction(PlayerDataList[i].FactionToRemove)
                EndIf
                If PlayerDataList[i].KeywordToAdd != None
                    player.AddKeyword(PlayerDataList[i].KeywordToAdd)
                EndIf
                If PlayerDataList[i].KeywordToRemove != None
                    player.RemoveKeyword(PlayerDataList[i].KeywordToRemove)
                EndIf
                If PlayerDataList[i].ActorValueToSet != None
                    player.SetValue(PlayerDataList[i].ActorValueToSet, PlayerDataList[i].ValueToSet as Float)
                EndIf
            EndIf
        EndIf
        i += 1
    EndWhile
EndEvent
