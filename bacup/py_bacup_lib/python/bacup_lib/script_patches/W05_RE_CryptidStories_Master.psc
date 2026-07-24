Event OnInit()
    CryptidList = new Int[4]
    CryptidList[0] = 0
    CryptidList[1] = 1
    CryptidList[2] = 2
    CryptidList[3] = 3
    ListLength = 4
    ChosenCryptid = CryptidList[Utility.RandomInt(0, ListLength - 1)]
    ShouldSpawn = 1
EndEvent
