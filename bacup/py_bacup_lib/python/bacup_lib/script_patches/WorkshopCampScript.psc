Event OnInit()
    If SQ_CampAttackKeyword != None
        Self.AddKeyword(SQ_CampAttackKeyword)
    EndIf
    myPlayer = Game.GetPlayer()
EndEvent
