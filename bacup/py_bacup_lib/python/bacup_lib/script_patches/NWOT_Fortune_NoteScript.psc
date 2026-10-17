; Fortune notes read "Your Lucky Nuka flavor is: <Token.Name=NukaFlavor>"; the fortune teller fills the token from
; NWOT_FortuneTeller_PossibleNukaFlavors before handing the note over.
Function SetLuckyFlavor()
    If NWOT_FortuneTeller_PossibleNukaFlavors == None || NWOT_FortuneTeller_PossibleNukaFlavors.GetSize() == 0
        Return
    EndIf
    Int pick = Utility.RandomInt(0, NWOT_FortuneTeller_PossibleNukaFlavors.GetSize() - 1)
    AddTextReplacementData("NukaFlavor", NWOT_FortuneTeller_PossibleNukaFlavors.GetAt(pick))
EndFunction
